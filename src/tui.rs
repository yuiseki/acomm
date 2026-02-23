use crate::protocol::ProtocolEvent;
use acore::AgentTool;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::error::Error;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode { Normal, Editing }

pub struct InputState {
    pub text: String,
    pub cursor_position: usize, // 文字数ベースのインデックス
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub kill_buffer: String,
}

impl InputState {
    pub fn new() -> Self {
        Self { 
            text: String::new(), 
            cursor_position: 0,
            history: Vec::new(),
            history_index: None,
            kill_buffer: String::new(),
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let count = self.text.chars().count();
        if self.cursor_position < count {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_up(&mut self) {
        let lines = self.get_lines();
        let (current_row, current_col) = self.get_cursor_coords();
        if current_row > 0 {
            let target_row = current_row - 1;
            let target_col = current_col.min(lines[target_row].chars().count());
            let mut new_pos = 0;
            for i in 0..target_row {
                new_pos += lines[i].chars().count() + 1;
            }
            new_pos += target_col;
            self.cursor_position = new_pos;
        }
    }

    pub fn move_cursor_down(&mut self) {
        let lines = self.get_lines();
        let (current_row, current_col) = self.get_cursor_coords();
        if current_row < lines.len() - 1 {
            let target_row = current_row + 1;
            let target_col = current_col.min(lines[target_row].chars().count());
            let mut new_pos = 0;
            for i in 0..target_row {
                new_pos += lines[i].chars().count() + 1;
            }
            new_pos += target_col;
            self.cursor_position = new_pos;
        }
    }

    pub fn enter_char(&mut self, new_char: char) {
        let idx = self.byte_index();
        self.text.insert(idx, new_char);
        self.cursor_position += 1;
    }

    fn byte_index(&self) -> usize {
        self.text
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor_position)
            .unwrap_or(self.text.len())
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position != 0 {
            self.move_cursor_left();
            let idx = self.byte_index();
            self.text.remove(idx);
        }
    }

    pub fn kill_line(&mut self) {
        let idx = self.byte_index();
        self.kill_buffer = self.text.split_off(idx);
    }

    pub fn yank(&mut self) {
        let yank_text = self.kill_buffer.clone();
        let idx = self.byte_index();
        self.text.insert_str(idx, &yank_text);
        self.cursor_position += yank_text.chars().count();
    }

    pub fn reset(&mut self) -> String {
        let res = self.text.clone();
        if !res.is_empty() {
            self.history.push(res.clone());
        }
        self.text.clear();
        self.cursor_position = 0;
        self.history_index = None;
        res
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        let new_idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        self.history_index = Some(new_idx);
        self.text = self.history[new_idx].clone();
        self.cursor_position = self.text.chars().count();
    }

    pub fn history_down(&mut self) {
        let Some(idx) = self.history_index else { return };
        if idx + 1 < self.history.len() {
            let new_idx = idx + 1;
            self.history_index = Some(new_idx);
            self.text = self.history[new_idx].clone();
        } else {
            self.history_index = None;
            self.text.clear();
        }
        self.cursor_position = self.text.chars().count();
    }

    pub fn get_lines(&self) -> Vec<String> {
        self.text.split('\n').map(|s| s.to_string()).collect()
    }

    pub fn get_cursor_coords(&self) -> (usize, usize) {
        let text_before: String = self.text.chars().take(self.cursor_position).collect();
        let lines: Vec<&str> = text_before.split('\n').collect();
        let row = lines.len() - 1;
        let col = lines.last().unwrap_or(&"").chars().count();
        (row, col)
    }
}

pub struct App {
    pub input: InputState,
    pub input_mode: InputMode,
    pub messages: Vec<String>,
    pub active_cli: AgentTool,
    pub is_processing: bool,
    pub scroll: u16,
    pub channel: String,
    pub spinner_idx: usize,
}

#[derive(Debug)]
pub enum AppEvent {
    Input(event::KeyEvent),
    BusEvent(ProtocolEvent),
    Tick,
}

pub async fn run_tui_app<B: Backend, W: AsyncWriteExt + Unpin>(
    terminal: &mut Terminal<B>,
    mut app: App,
    writer: &mut W,
    mut rx: mpsc::Receiver<AppEvent>,
) -> Result<(), Box<dyn Error>> 
where <B as Backend>::Error: 'static {
    loop {
        terminal.draw(|f| render_ui(f, &mut app))?;

        if let Some(event) = rx.recv().await {
            match event {
                AppEvent::Tick => {
                    if app.is_processing {
                        app.spinner_idx = (app.spinner_idx + 1) % 10;
                    }
                }
                AppEvent::BusEvent(bus_event) => match bus_event {
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
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c') => return Ok(()),
                            KeyCode::Char('p') => app.input.history_up(),
                            KeyCode::Char('n') => app.input.history_down(),
                            KeyCode::Char('k') => app.input.kill_line(),
                            KeyCode::Char('y') => app.input.yank(),
                            KeyCode::Char('a') => app.input.cursor_position = 0,
                            KeyCode::Char('e') => app.input.cursor_position = app.input.text.chars().count(),
                            _ => {}
                        }
                    }

                    match app.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('i') => app.input_mode = InputMode::Editing,
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('1') | KeyCode::Char('2') | KeyCode::Char('3') | KeyCode::Char('4') => {
                                let tool_name = match key.code {
                                    KeyCode::Char('1') => "gemini",
                                    KeyCode::Char('2') => "claude",
                                    KeyCode::Char('3') => "codex",
                                    _ => "opencode",
                                };
                                let event = ProtocolEvent::Prompt { text: format!("/tool {tool_name}"), tool: None, channel: None };
                                if let Ok(j) = serde_json::to_string(&event) { let _ = writer.write_all(format!("{}\n", j).as_bytes()).await; }
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.scroll = app.scroll.saturating_sub(1),
                            KeyCode::Down | KeyCode::Char('j') => app.scroll = app.scroll.saturating_add(1),
                            _ => {}
                        }
                        InputMode::Editing => match key.code {
                            KeyCode::Enter => {
                                if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::ALT) {
                                    app.input.enter_char('\n');
                                } else {
                                    let msg = app.input.reset();
                                    if !msg.is_empty() {
                                        let event = ProtocolEvent::Prompt { text: msg, tool: None, channel: Some(app.channel.clone()) };
                                        if let Ok(j) = serde_json::to_string(&event) { let _ = writer.write_all(format!("{}\n", j).as_bytes()).await; }
                                    }
                                }
                            }
                            KeyCode::Char(c) => if !key.modifiers.contains(KeyModifiers::CONTROL) { app.input.enter_char(c); }
                            KeyCode::Backspace => app.input.delete_char(),
                            KeyCode::Left => app.input.move_cursor_left(),
                            KeyCode::Right => app.input.move_cursor_right(),
                            KeyCode::Up => app.input.move_cursor_up(),
                            KeyCode::Down => app.input.move_cursor_down(),
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

fn render_ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(5)]).split(f.area());
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mode_str = if app.is_processing { format!("THINKING {}", spinner_chars[app.spinner_idx]) } else { match app.input_mode { InputMode::Normal => "NORMAL".into(), InputMode::Editing => "INSERT".into() } };
    let header = Paragraph::new(format!(" Mode: {} | CLI: {} | Channel: {}", mode_str, app.active_cli.command_name(), app.channel)).block(Block::default().title(" Status ").borders(Borders::ALL));
    f.render_widget(header, chunks[0]);
    let chat_height = chunks[1].height.saturating_sub(2);
    let current_scroll = app.scroll.min(app.messages.len().saturating_sub(chat_height as usize) as u16);
    let chat = Paragraph::new(app.messages.join("\n")).wrap(Wrap { trim: true }).scroll((current_scroll, 0)).block(Block::default().title(" Chat history ").borders(Borders::ALL));
    f.render_widget(chat, chunks[1]);
    let input = Paragraph::new(app.input.text.as_str()).style(if let InputMode::Editing = app.input_mode { Style::default().fg(Color::Yellow) } else { Style::default() }).block(Block::default().title(" Input ").borders(Borders::ALL));
    f.render_widget(input, chunks[2]);
    if let (InputMode::Editing, false) = (app.input_mode, app.is_processing) {
        let (row, _col) = app.input.get_cursor_coords();
        let text_before_cursor: String = app.input.text.chars().take(app.input.cursor_position).collect();
        let cursor_x: u16 = text_before_cursor.split('\n').last().unwrap_or("").width() as u16;
        f.set_cursor_position((chunks[2].x + cursor_x + 1, chunks[2].y + row as u16 + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_input_state_complex() {
        let mut input = InputState::new();
        input.enter_char('a');
        input.enter_char('b');
        input.move_cursor_left();
        input.enter_char('c');
        assert_eq!(input.text, "acb");
        input.kill_line(); // kill "b"
        assert_eq!(input.text, "ac");
        input.yank();
        assert_eq!(input.text, "acb");
    }
}
