use crate::protocol::ProtocolEvent;
use acore::AgentProvider;
use crossterm::event::{self, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{error::Error, fs, path::PathBuf};
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode { Normal, Editing }

pub struct InputState {
    pub text: String,
    pub cursor_position: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub kill_buffer: String,
}

impl InputState {
    pub fn new() -> Self {
        let mut history = Vec::new();
        if let Some(path) = Self::history_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    history = content.lines().map(|s| s.to_string()).collect();
                }
            }
        }
        Self { 
            text: String::new(), 
            cursor_position: 0,
            history,
            history_index: None,
            kill_buffer: String::new(),
        }
    }

    fn history_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|mut p| {
            p.push("acomm");
            p.push("history.txt");
            p
        })
    }

    fn save_history(&self) {
        if let Some(path) = Self::history_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let content = self.history.join("\n");
            let _ = fs::write(path, content);
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
            if self.history.last() != Some(&res) {
                self.history.push(res.clone());
                self.save_history();
            }
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
    pub active_cli: AgentProvider,
    pub is_processing: bool,
    pub scroll: u16,
    pub auto_scroll: bool,
    pub channel: String,
    pub spinner_idx: usize,
}

impl App {
    pub fn handle_bus_event(&mut self, event: ProtocolEvent) {
        match event {
            ProtocolEvent::SyncContext { context } => {
                self.messages.push("--- Today's Context ---\n".into());
                self.messages.extend(context.lines().map(|s| format!("{s}\n")));
                self.messages.push("-----------------------\n".into());
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
            ProtocolEvent::Prompt { text, channel, .. } => {
                let channel_name = channel.unwrap_or_else(|| "unknown".into());
                let msg = format!("[user][{}] {}\n", channel_name, text);
                if self.messages.last() != Some(&msg) {
                    self.messages.push("--- (Start) ---\n".into());
                    self.messages.push(msg);
                }
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
            ProtocolEvent::AgentChunk { chunk, .. } => {
                if chunk.is_empty() { return; }
                let tool_prefix = format!("[{}] ", self.active_cli.command_name());
                
                for line in chunk.split_inclusive('\n') {
                    let mut pushed = false;
                    if let Some(last) = self.messages.last_mut() {
                        if last.starts_with(&tool_prefix) && !last.ends_with('\n') {
                            last.push_str(line);
                            pushed = true;
                        }
                    }
                    if !pushed {
                        let is_just_nl = line == "\n";
                        let prev_is_just_prefix = self.messages.last().map_or(false, |m| m == &format!("{tool_prefix}\n"));
                        if is_just_nl && prev_is_just_prefix {
                            // Skip redundant
                        } else {
                            self.messages.push(format!("{tool_prefix}{line}"));
                        }
                    }
                }
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
            ProtocolEvent::StatusUpdate { is_processing, .. } => { 
                self.is_processing = is_processing; 
            }
            ProtocolEvent::ProviderSwitched { provider } => { 
                self.active_cli = provider; 
            }
            ProtocolEvent::SystemMessage { msg, .. } => { 
                self.messages.push(format!("[System]: {}\n", msg)); 
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
            ProtocolEvent::AgentDone { .. } => {
                self.is_processing = false;
                if let Some(last) = self.messages.last_mut() {
                    if !last.ends_with('\n') { last.push('\n'); }
                }
                self.messages.push("--- (Done) ---\n".into());
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
            ProtocolEvent::ModelSwitched { model } => {
                self.messages.push(format!("[Model switched → {}]\n", model));
                if self.auto_scroll { self.scroll_to_bottom(); }
            }
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        let total_lines = self.messages.iter().map(|m| m.chars().filter(|&c| c == '\n').count()).sum::<usize>();
        self.scroll = total_lines as u16;
    }
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
                AppEvent::BusEvent(bus_event) => {
                    app.handle_bus_event(bus_event);
                }
                AppEvent::Input(key) => {
                    // keyboard enhancement が有効のとき Press/Release/Repeat 全て届くため、
                    // Press のみを処理する
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
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
                                let event = ProtocolEvent::Prompt { text: format!("/tool {tool_name}"), provider: None, channel: None };
                                if let Ok(j) = serde_json::to_string(&event) { let _ = writer.write_all(format!("{}\n", j).as_bytes()).await; }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.scroll = app.scroll.saturating_sub(1);
                                app.auto_scroll = false;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.scroll = app.scroll.saturating_add(1);
                                // 最下部に達したら自動スクロール復帰
                                let total_lines = app.messages.iter().map(|m| m.chars().filter(|&c| c == '\n').count()).sum::<usize>() as u16;
                                if app.scroll >= total_lines { app.auto_scroll = true; }
                            }
                            KeyCode::PageUp => {
                                app.scroll = app.scroll.saturating_sub(10);
                                app.auto_scroll = false;
                            }
                            KeyCode::PageDown => {
                                app.scroll = app.scroll.saturating_add(10);
                                let total_lines = app.messages.iter().map(|m| m.chars().filter(|&c| c == '\n').count()).sum::<usize>() as u16;
                                if app.scroll >= total_lines { app.auto_scroll = true; }
                            }
                            _ => {}
                        }
                        InputMode::Editing => match key.code {
                            KeyCode::Enter => {
                                if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::ALT) {
                                    app.input.enter_char('\n');
                                } else {
                                    let msg = app.input.reset();
                                    if !msg.is_empty() {
                                        app.messages.push("--- (Start) ---\n".into());
                                        app.messages.push(format!("[user][{}] {}\n", app.channel, msg));
                                        app.is_processing = true;
                                        app.auto_scroll = true; // 自身の入力時は最下部へ
                                        app.scroll_to_bottom();
                                        
                                        let event = ProtocolEvent::Prompt { text: msg, provider: None, channel: Some(app.channel.clone()) };
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
        }
    }
}

/// 入力テキストの行数に応じて入力エリアの高さを計算する（borders 込み、最小 5）
pub fn compute_input_height(text: &str) -> u16 {
    let line_count = text.split('\n').count() as u16;
    (line_count + 2).max(5)
}

fn render_ui(f: &mut Frame, app: &mut App) {
    let input_height = compute_input_height(&app.input.text);
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(input_height)]).split(f.area());
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mode_str = if app.is_processing { format!("THINKING {}", spinner_chars[app.spinner_idx]) } else { match app.input_mode { InputMode::Normal => "NORMAL".into(), InputMode::Editing => "INSERT".into() } };
    let header = Paragraph::new(format!(" Mode: {} | CLI: {} | Channel: {} | AutoScroll: {}", mode_str, app.active_cli.command_name(), app.channel, app.auto_scroll)).block(Block::default().title(" Status ").borders(Borders::ALL));
    f.render_widget(header, chunks[0]);
    
    let chat_height = chunks[1].height.saturating_sub(2);
    let chat_content = app.messages.join("");
    let total_lines = chat_content.chars().filter(|&c| c == '\n').count();
    let current_scroll = app.scroll.min(total_lines.saturating_sub(chat_height as usize) as u16);
    
    let chat = Paragraph::new(chat_content).wrap(Wrap { trim: false }).scroll((current_scroll, 0)).block(Block::default().title(" Chat history ").borders(Borders::ALL));
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
    fn test_compute_input_height_single_line() {
        assert_eq!(compute_input_height(""), 5);
        assert_eq!(compute_input_height("hello"), 5);
        assert_eq!(compute_input_height("一行のテキスト"), 5);
    }

    #[test]
    fn test_compute_input_height_multiline() {
        // 2行: max(2+2, 5) = 5
        assert_eq!(compute_input_height("line1\nline2"), 5);
        // 3行: max(3+2, 5) = 5
        assert_eq!(compute_input_height("a\nb\nc"), 5);
        // 4行: max(4+2, 5) = 6
        assert_eq!(compute_input_height("a\nb\nc\nd"), 6);
        // 5行: max(5+2, 5) = 7
        assert_eq!(compute_input_height("a\nb\nc\nd\ne"), 7);
    }

    #[test]
    fn test_newline_in_input_state() {
        let mut input = InputState::new();
        input.enter_char('a');
        input.enter_char('\n');
        input.enter_char('b');
        let lines = input.get_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "a");
        assert_eq!(lines[1], "b");
    }

    #[test]
    fn test_cursor_coords_after_newline() {
        let mut input = InputState::new();
        input.enter_char('h');
        input.enter_char('i');
        input.enter_char('\n');
        input.enter_char('x');
        // カーソルは row=1, col=1 にあるはず
        let (row, col) = input.get_cursor_coords();
        assert_eq!(row, 1);
        assert_eq!(col, 1);
    }

    #[test]
    fn test_input_state_complex() {
        let mut input = InputState::new();
        input.enter_char('a');
        input.enter_char('b');
        input.move_cursor_left();
        input.enter_char('c');
        assert_eq!(input.text, "acb");
        input.kill_line();
        assert_eq!(input.text, "ac");
        input.yank();
        assert_eq!(input.text, "acb");
    }

    #[test]
    fn test_app_message_handling_clean_output() {
        let mut app = App {
            input: InputState::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            active_cli: AgentProvider::Gemini,
            is_processing: false,
            scroll: 0,
            auto_scroll: true,
            channel: "tui".into(),
            spinner_idx: 0,
        };

        app.handle_bus_event(ProtocolEvent::Prompt { text: "test".into(), provider: None, channel: Some("tui".into()) });
        app.handle_bus_event(ProtocolEvent::AgentChunk { chunk: "Line 1\n".into(), channel: Some("tui".into()) });
        app.handle_bus_event(ProtocolEvent::AgentChunk { chunk: "\n".into(), channel: Some("tui".into()) });
        app.handle_bus_event(ProtocolEvent::AgentChunk { chunk: "\n".into(), channel: Some("tui".into()) });
        app.handle_bus_event(ProtocolEvent::AgentChunk { chunk: "Line 3".into(), channel: Some("tui".into()) });
        app.handle_bus_event(ProtocolEvent::AgentDone { channel: Some("tui".into()) });

        for (i, m) in app.messages.iter().enumerate() {
            println!("msg[{}]: {:?}", i, m);
        }

        let empty_gemini_lines = app.messages.iter().filter(|m| m.as_str() == "[gemini] \n" || m.as_str() == "[gemini] ").count();
        assert!(empty_gemini_lines <= 1, "Too many redundant empty gemini lines found");
    }
}
