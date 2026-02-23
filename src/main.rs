mod protocol;
mod bridge;

use acore::AgentTool;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use protocol::ProtocolEvent;
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
}

#[derive(Clone, Copy)]
enum InputMode { Normal, Editing }

#[derive(Debug)]
enum AppEvent {
    Input(event::KeyEvent),
    BusEvent(ProtocolEvent),
}

struct App {
    input: String,
    input_mode: InputMode,
    messages: Vec<String>,
    active_cli: AgentTool,
    is_processing: bool,
    scroll: u16,
    channel: String,
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
        text: msg.to_string(), 
        tool: None, 
        channel: channel.map(|s| s.to_string()) 
    };
    let j = serde_json::to_string(&event)?;
    stream.write_all(format!("{}\n", j).as_bytes()).await?;
    let _ = stream.shutdown().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn start_dump() -> Result<(), Box<dyn Error>> {
    let stream = ensure_bridge_connection(false).await?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    
    let mut tool = "bot".to_string();
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    display_event(&event, &mut tool)?;
                }
            }
            _ => break,
        }
    }
    let _ = writer.shutdown().await;
    Ok(())
}

fn display_event(event: &ProtocolEvent, active_tool_name: &mut String) -> io::Result<()> {
    match event {
        ProtocolEvent::Prompt { text, channel, .. } => {
            println!("\n--- (Start) ---");
            println!("[user][{}] {}", channel.as_deref().unwrap_or("unknown"), text);
        }
        ProtocolEvent::AgentLine { line } => {
            println!("[{}] {}", active_tool_name, line);
        }
        ProtocolEvent::AgentDone => {
            println!("--- (Done) ---");
        }
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
    let mut lines = BufReader::new(stream).lines();
    
    let mut active_tool_name = "bot".to_string();
    let mut is_thinking = false;
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut spinner_idx = 0;

    println!("--- Subscribed to acomm bridge ---");

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line? {
                    Some(l) => l,
                    None => break,
                };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    if matches!(event, ProtocolEvent::StatusUpdate { is_processing: true }) {
                        is_thinking = true;
                    } else if matches!(event, ProtocolEvent::StatusUpdate { is_processing: false } | ProtocolEvent::AgentLine { .. } | ProtocolEvent::AgentDone) {
                        if is_thinking {
                            print!("\r\x1B[K"); // Clear Thinking line
                            is_thinking = false;
                        }
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

    let (tx, mut rx) = mpsc::channel(100);
    let mut app = App {
        input: String::new(), input_mode: InputMode::Normal, messages: Vec::new(),
        active_cli: acore::AgentTool::Gemini, is_processing: false, scroll: 0,
        channel: channel.unwrap_or("tui").to_string(),
    };

    let tx_bridge = tx.clone();
    let bridge_handle = tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                if let Err(_) = tx_bridge.send(AppEvent::BusEvent(event)).await {
                    break;
                }
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

    let _ = run_app(&mut terminal, &mut app, &mut writer, &mut rx).await;

    bridge_handle.abort(); input_handle.abort();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run_app<B: Backend, W: AsyncWriteExt + Unpin>(
    terminal: &mut Terminal<B>, app: &mut App, writer: &mut W, rx: &mut mpsc::Receiver<AppEvent>,
) -> Result<(), Box<dyn Error>> 
where <B as Backend>::Error: 'static {
    loop {
        terminal.draw(|f| ui(f, app))?;
        if let Some(event) = rx.recv().await {
            match event {
                AppEvent::BusEvent(event) => match event {
                    ProtocolEvent::SyncContext { context } => {
                        app.messages.push("--- Today's Context ---".into());
                        app.messages.extend(context.lines().map(|s| s.into()));
                        app.messages.push("-----------------------".into());
                    }
                    ProtocolEvent::Prompt { text, channel, .. } => {
                        app.messages.push("--- (Start) ---".into());
                        app.messages.push(format!("[user][{}] {}", channel.unwrap_or_else(|| "unknown".into()), text));
                    }
                    ProtocolEvent::AgentLine { line } => {
                        app.messages.push(format!("[{}] {}", app.active_cli.command_name(), line));
                    }
                    ProtocolEvent::StatusUpdate { is_processing } => { app.is_processing = is_processing; }
                    ProtocolEvent::ToolSwitched { tool } => { app.active_cli = tool; }
                    ProtocolEvent::SystemMessage { msg, .. } => { app.messages.push(format!("[System]: {}", msg)); }
                    ProtocolEvent::AgentDone => {
                        app.is_processing = false;
                        app.messages.push("--- (Done) ---".into());
                    }
                }
                AppEvent::Input(key) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') { return Ok(()); }
                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('i') => app.input_mode = InputMode::Editing,
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('1') => {
                                let event = ProtocolEvent::Prompt { text: "/tool gemini".into(), tool: None, channel: None };
                                if let Ok(j) = serde_json::to_string(&event) {
                                    let _ = writer.write_all(format!("{}\n", j).as_bytes()).await;
                                }
                            }
                            KeyCode::Char('2') => {
                                let event = ProtocolEvent::Prompt { text: "/tool claude".into(), tool: None, channel: None };
                                if let Ok(j) = serde_json::to_string(&event) {
                                    let _ = writer.write_all(format!("{}\n", j).as_bytes()).await;
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.scroll = app.scroll.saturating_sub(1),
                            KeyCode::Down | KeyCode::Char('j') => app.scroll = app.scroll.saturating_add(1),
                            _ => {}
                        }
                        InputMode::Editing => match key.code {
                            KeyCode::Enter => {
                                let msg = app.input.drain(..).collect::<String>();
                                if !msg.is_empty() {
                                    let event = ProtocolEvent::Prompt { text: msg, tool: None, channel: Some(app.channel.clone()) };
                                    if let Ok(j) = serde_json::to_string(&event) {
                                        let _ = writer.write_all(format!("{}\n", j).as_bytes()).await;
                                    }
                                }
                            }
                            KeyCode::Char(c) => app.input.push(c),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Esc => app.input_mode = InputMode::Normal,
                            _ => {}
                        }
                    }
                }
            }
            app.scroll = app.messages.len().saturating_sub(1) as u16;
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(5)]).split(f.area());
    let mode_str = if app.is_processing { "THINKING..." } else { match app.input_mode { InputMode::Normal => "NORMAL", InputMode::Editing => "INSERT" } };
    let header = Paragraph::new(format!(" Mode: {} | CLI: {} | Channel: {}", mode_str, app.active_cli.command_name(), app.channel)).block(Block::default().title(" Status ").borders(Borders::ALL));
    f.render_widget(header, chunks[0]);
    let chat_height = chunks[1].height.saturating_sub(2);
    let current_scroll = app.scroll.min(app.messages.len().saturating_sub(chat_height as usize) as u16);
    let chat = Paragraph::new(app.messages.join("\n")).wrap(Wrap { trim: true }).scroll((current_scroll, 0)).block(Block::default().title(" Chat ").borders(Borders::ALL));
    f.render_widget(chat, chunks[1]);
    let input = Paragraph::new(app.input.as_str()).style(if let InputMode::Editing = app.input_mode { Style::default().fg(Color::Yellow) } else { Style::default() }).block(Block::default().title(" Input ").borders(Borders::ALL));
    f.render_widget(input, chunks[2]);
    if let (InputMode::Editing, false) = (app.input_mode, app.is_processing) {
        let lines: Vec<&str> = app.input.split('\n').collect();
        let last_line = lines.last().unwrap_or(&"");
        f.set_cursor_position((chunks[2].x + last_line.chars().count() as u16 + 1, chunks[2].y + lines.len() as u16));
    }
}
