mod protocol;
mod bridge;
mod tui;
mod slack;
mod ntfy;
mod discord;

use acore::AgentProvider;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use protocol::ProtocolEvent;
use tui::{App, AppEvent, InputMode, InputState};
use std::{error::Error, io, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    #[arg(short, long)] bridge: bool,
    #[arg(short, long)] publish: Option<String>,
    #[arg(short, long)] channel: Option<String>,
    #[arg(short, long, alias = "s")] subscribe: bool,
    #[arg(short, long)] dump: bool,
    #[arg(short, long)] reset: bool,
    #[arg(long)] slack: bool,
    #[arg(long)] ntfy: bool,
    #[arg(long)] discord: bool,
    /// エージェントとしてメッセージを送信する (--discord / --slack / --ntfy で送信先を指定)
    #[arg(long)] agent: Option<String>,
}

const SOCKET_PATH: &str = "/tmp/acomm.sock";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();
    if args.bridge { return bridge::start_bridge().await; }

    // --agent: send a proactive message as the bot without going through the AI pipeline.
    // --discord / --slack / --ntfy narrow the targets; omitting all sends to every
    // configured adapter (silently skips those whose env vars are missing).
    if let Some(ref text) = args.agent {
        let any_target = args.discord || args.slack || args.ntfy;
        if !any_target || args.discord {
            match discord::notify_discord(text).await {
                Ok(()) => println!("Discord: sent."),
                Err(e) if any_target => return Err(e),
                Err(e) => eprintln!("Discord: skipped ({})", e),
            }
        }
        if !any_target || args.slack {
            match slack::notify_slack(text).await {
                Ok(()) => println!("Slack: sent."),
                Err(e) if any_target => return Err(e),
                Err(e) => eprintln!("Slack: skipped ({})", e),
            }
        }
        if !any_target || args.ntfy {
            match ntfy::notify_ntfy(text).await {
                Ok(()) => println!("ntfy: sent."),
                Err(e) if any_target => return Err(e),
                Err(e) => eprintln!("ntfy: skipped ({})", e),
            }
        }
        return Ok(());
    }

    if args.reset { return publish_to_bridge("/clear", Some("bridge")).await; }
    if args.slack { return slack::start_slack_adapter().await; }
    if args.ntfy { return ntfy::start_ntfy_adapter().await; }
    if args.discord { return discord::start_discord_adapter().await; }
    if let Some(mut msg) = args.publish {
        if msg == "-" {
            let mut buffer = String::new();
            tokio::io::stdin().read_to_string(&mut buffer).await?;
            msg = buffer;
        }
        return publish_to_bridge(&msg, args.channel.as_deref()).await;
    }
    if args.dump { return start_dump().await; }
    if args.subscribe { return start_subscribe().await; }
    start_tui(args.channel.as_deref()).await
}

async fn ensure_bridge_connection(auto_start: bool) -> Result<UnixStream, Box<dyn Error>> {
    if !auto_start {
        return UnixStream::connect(SOCKET_PATH).await.map_err(|e| format!("Bridge not running: {e}").into());
    }
    for _ in 0..3 {
        match UnixStream::connect(SOCKET_PATH).await {
            Ok(s) => return Ok(s),
            Err(_) => {
                if Path::new(SOCKET_PATH).exists() { let _ = std::fs::remove_file(SOCKET_PATH); }
                let exe = std::env::current_exe()?;
                let _ = std::process::Command::new(exe).arg("--bridge").spawn();
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    Err("Failed to start or connect to bridge.".into())
}

async fn publish_to_bridge(msg: &str, channel: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut stream = ensure_bridge_connection(false).await?;
    let event = ProtocolEvent::Prompt { text: msg.to_string(), provider: None, channel: channel.map(|s| s.to_string()) };
    let j = serde_json::to_string(&event)?;
    stream.write_all(format!("{}\n", j).as_bytes()).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

async fn start_dump() -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(false).await?;
    let mut lines = BufReader::new(stream).lines();
    let mut provider = "bot".to_string();
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    display_event(&event, &mut provider, &mut true)?;
                }
            }
            _ => break,
        }
    }
    Ok(())
}

fn display_event(event: &ProtocolEvent, active_provider_name: &mut String, is_start_of_line: &mut bool) -> io::Result<()> {
    match event {
        ProtocolEvent::Prompt { text, channel, .. } => {
            println!("\n--- (Start) ---");
            println!("[user][{}] {}", channel.as_deref().unwrap_or("unknown"), text);
            *is_start_of_line = true;
        }
        ProtocolEvent::AgentChunk { chunk, .. } => {
            for line in chunk.split_inclusive('\n') {
                if *is_start_of_line {
                    print!("[{}] ", active_provider_name);
                    *is_start_of_line = false;
                }
                print!("{line}");
                if line.ends_with('\n') { *is_start_of_line = true; }
            }
        }
        ProtocolEvent::AgentDone { .. } => {
            if !*is_start_of_line { println!(); }
            *is_start_of_line = true;
        }
        ProtocolEvent::ProviderSwitched { provider } => {
            *active_provider_name = provider.command_name().to_string();
            println!("\n[System]: Active provider switched to {}", active_provider_name);
            *is_start_of_line = true;
        }
        ProtocolEvent::SystemMessage { msg, channel } => {
            println!("\n[System ({})]: {}", channel.as_deref().unwrap_or("bridge"), msg);
            *is_start_of_line = true;
        }
        _ => {}
    }
    io::Write::flush(&mut io::stdout())?;
    Ok(())
}

async fn start_subscribe() -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(false).await?;
    let mut lines = BufReader::new(stream).lines();
    let mut active_provider_name = "bot".to_string();
    let mut is_thinking = false;
    let mut is_start_of_line = true;
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_idx = 0;
    println!("--- Subscribed to acomm bridge ---");
    loop {
        tokio::select! {
            line_res = lines.next_line() => {
                let line = match line_res? { Some(l) => l, None => break };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    if matches!(event, ProtocolEvent::StatusUpdate { is_processing: true, .. }) { is_thinking = true; }
                    else if matches!(event, ProtocolEvent::StatusUpdate { is_processing: false, .. } | ProtocolEvent::AgentChunk { .. } | ProtocolEvent::AgentDone { .. }) {
                        if is_thinking { print!("\r\x1B[K"); is_thinking = false; }
                    }
                    display_event(&event, &mut active_provider_name, &mut is_start_of_line)?;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)), if is_thinking => {
                spinner_idx = (spinner_idx + 1) % spinner_chars.len();
                print!("\r[Status] Thinking {}", spinner_chars[spinner_idx]);
                io::Write::flush(&mut io::stdout())?;
            }
        }
    }
    Ok(())
}

async fn start_tui(channel: Option<&str>) -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(true).await?;
    let (reader, mut writer) = tokio::io::split(stream);
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let _ = execute!(stdout, EnterAlternateScreen, EnableMouseCapture);
    // Kitty keyboard protocol を有効化して Shift+Enter などの修飾キーを区別できるようにする。
    // 対応していないターミナルでは失敗するが graceful に継続する。
    let keyboard_enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if keyboard_enhanced {
        let _ = execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS,
            )
        );
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let (tx, rx) = mpsc::channel(100);
    let app = App {
        input: InputState::new(), input_mode: InputMode::Normal, messages: Vec::new(),
        active_cli: AgentProvider::Gemini, is_processing: false, scroll: 0,
        auto_scroll: true,
        channel: channel.unwrap_or("tui").to_string(), spinner_idx: 0,
    };
    let tx_bridge = tx.clone();
    let bridge_handle = tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                let _ = tx_bridge.send(AppEvent::BusEvent(event)).await;
            }
        }
    });
    let tx_keys = tx.clone();
    let input_handle = tokio::spawn(async move {
        loop {
            if event::poll(std::time::Duration::from_millis(16)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    let _ = tx_keys.send(AppEvent::Input(key)).await;
                }
            }
        }
    });
    let tx_tick = tx.clone();
    let tick_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if let Err(_) = tx_tick.send(AppEvent::Tick).await { break; }
        }
    });
    let _ = tui::run_tui_app(&mut terminal, app, &mut writer, rx).await;
    bridge_handle.abort(); input_handle.abort(); tick_handle.abort();
    disable_raw_mode()?;
    if keyboard_enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
