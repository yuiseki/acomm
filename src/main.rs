mod protocol;
mod bridge;
mod tui;
mod slack;

use acore::AgentTool;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
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
    /// Run as a background bridge process
    #[arg(short, long)]
    bridge: bool,

    /// Send a message to the bridge
    #[arg(short, long)]
    publish: Option<String>,

    /// Source channel name
    #[arg(short, long)]
    channel: Option<String>,

    /// Subscribe to bridge events and print to stdout (tail -f style)
    #[arg(short, long, alias = "s")]
    subscribe: bool,

    /// Dump current backlog and exit
    #[arg(short, long)]
    dump: bool,

    /// Reset bridge backlog and exit
    #[arg(short, long)]
    reset: bool,

    /// Run as a Slack Socket Mode adapter
    #[arg(long)]
    slack: bool,
}

const SOCKET_PATH: &str = "/tmp/acomm.sock";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();

    if args.bridge {
        return bridge::start_bridge().await;
    }

    if args.reset {
        println!("Resetting bridge backlog...");
        return publish_to_bridge("/clear", Some("bridge")).await;
    }

    if args.slack {
        return slack::start_slack_adapter().await;
    }

    if let Some(mut msg) = args.publish {
        if msg == "-" {
            let mut buffer = String::new();
            tokio::io::stdin().read_to_string(&mut buffer).await?;
            msg = buffer;
        }
        return publish_to_bridge(&msg, args.channel.as_deref()).await;
    }

    if args.dump {
        return start_dump().await;
    }

    if args.subscribe {
        return start_subscribe().await;
    }

    start_tui(args.channel.as_deref()).await
}

async fn ensure_bridge_connection(auto_start: bool) -> Result<UnixStream, Box<dyn Error>> {
    if !auto_start {
        return UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
            format!("Bridge is not running. Error: {}", e).into()
        });
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
    Err("Failed to connect to bridge.".into())
}

async fn publish_to_bridge(msg: &str, channel: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut stream = ensure_bridge_connection(false).await?;
    let event = ProtocolEvent::Prompt { 
        text: msg.to_string(), tool: None, channel: channel.map(|s| s.to_string()) 
    };
    let j = serde_json::to_string(&event)?;
    stream.write_all(format!("{}\n", j).as_bytes()).await?;
    let _ = stream.shutdown().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn start_dump() -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(false).await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    let mut tool = "bot".to_string();
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), lines.next_line()).await {
            Ok(Ok(Some(l))) => {
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&l) {
                    display_event(&event, &mut tool)?;
                }
            }
            _ => break,
        }
    }
    Ok(())
}

fn display_event(event: &ProtocolEvent, active_tool_name: &mut String) -> io::Result<()> {
    match event {
        ProtocolEvent::Prompt { text, channel, .. } => {
            println!("\n--- (Start) ---");
            println!("[user][{}] {}", channel.as_deref().unwrap_or("unknown"), text);
        }
        ProtocolEvent::AgentLine { line } => { println!("[{}] {}", active_tool_name, line); }
        ProtocolEvent::AgentDone => { println!("--- (Done) ---"); }
        ProtocolEvent::ToolSwitched { tool } => {
            *active_tool_name = tool.command_name().to_string();
            println!("\n[System]: Active tool switched to {}", active_tool_name);
        }
        ProtocolEvent::SystemMessage { msg, channel } => {
            println!("\n[System ({})]: {}", channel.as_deref().unwrap_or("bridge"), msg);
        }
        _ => {}
    }
    io::Write::flush(&mut io::stdout())?;
    Ok(())
}

async fn start_subscribe() -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(false).await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    
    let mut active_tool_name = "bot".to_string();
    let mut is_thinking = false;
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_idx = 0;
    println!("--- Subscribed to acomm bridge ---");
    loop {
        tokio::select! {
            line_res = lines.next_line() => {
                let l = match line_res? { Some(line) => line, None => break };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&l) {
                    if matches!(event, ProtocolEvent::StatusUpdate { is_processing: true }) { is_thinking = true; }
                    else if matches!(event, ProtocolEvent::StatusUpdate { is_processing: false } | ProtocolEvent::AgentLine { .. } | ProtocolEvent::AgentDone) {
                        if is_thinking { print!("\r\x1B[K"); is_thinking = false; }
                    }
                    display_event(&event, &mut active_tool_name)?;
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
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let (tx, rx) = mpsc::channel(100);
    
    let app = App {
        input: InputState::new(), input_mode: InputMode::Normal, messages: Vec::new(),
        active_cli: AgentTool::Gemini, is_processing: false, scroll: 0,
        channel: channel.unwrap_or("tui").to_string(),
        spinner_idx: 0,
    };

    let tx_bridge = tx.clone();
    let bridge_handle = tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&l) {
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
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
