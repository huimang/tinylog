use crate::{
    config::ViewerConfig,
    session::{InteractiveViewerSession, RenderedFrame, RenderedRow, RowFocus, ViewerSession},
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, Write};

const LINE_JUMP_COMMAND_PREFIX: &str = ":";
const LINE_NUMBER_COLOR: Color = Color::Blue;
const CURRENT_MARKER_COLOR: Color = Color::Rgb {
    r: 255,
    g: 196,
    b: 128,
};
const FOCUS_MARKER_OFFSET: &str = " ";
const CURRENT_ROW_MARKER: &str = "▪";
const INACTIVE_ROW_MARKER: &str = " ";
const CONTENT_PADDING: &str = "";

/// Coordinates top-level viewer behavior for the CLI entrypoint.
#[derive(Debug, Clone)]
pub struct ViewerApplication {
    config: ViewerConfig,
}

impl ViewerApplication {
    /// Creates an application instance from a user-facing configuration.
    pub fn new(config: ViewerConfig) -> Self {
        Self { config }
    }

    /// Returns the startup banner shown by the scaffold CLI.
    pub fn banner(&self) -> String {
        match self.config.log_file.as_deref() {
            Some(path) => format!("Tinylog Viewer scaffold initialized for {path}."),
            None => "Tinylog Viewer scaffold initialized.".to_string(),
        }
    }

    /// Opens the configured file and enters an interactive browsing loop.
    pub fn run(&self) -> Result<(), String> {
        let Some(log_file) = self.config.log_file.clone() else {
            println!("{}", self.banner());
            return Ok(());
        };

        let mut session = InteractiveViewerSession::open(log_file, self.config.page_size)?;
        let mut stdout = io::stdout();
        terminal::enable_raw_mode()
            .map_err(|error| format!("failed to enable raw mode: {error}"))?;
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)
            .map_err(|error| format!("failed to enter alternate screen: {error}"))?;

        let result = self.run_loop(&mut session, &mut stdout);

        let cleanup_result = execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)
            .map_err(|error| format!("failed to leave alternate screen: {error}"));
        let raw_mode_result = terminal::disable_raw_mode()
            .map_err(|error| format!("failed to disable raw mode: {error}"));

        result?;
        cleanup_result?;
        raw_mode_result?;
        Ok(())
    }

    /// Runs the blocking event loop until the user exits.
    fn run_loop(
        &self,
        session: &mut InteractiveViewerSession,
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let mut command_buffer: Option<String> = None;
        let mut status_message: Option<String> = None;
        loop {
            let (width, height) = terminal::size()
                .map_err(|error| format!("failed to query terminal size: {error}"))?;
            self.render(
                session,
                usize::from(height),
                usize::from(width),
                command_buffer.as_deref(),
                status_message.as_deref(),
                stdout,
            )?;
            let event =
                event::read().map_err(|error| format!("failed to read key event: {error}"))?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if command_buffer.is_some() {
                    self.handle_command_key(
                        session,
                        key.code,
                        &mut command_buffer,
                        &mut status_message,
                    )?;
                    continue;
                }
                status_message = None;
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char(':') => {
                        command_buffer = Some(":".to_string());
                    }
                    KeyCode::Char('j') | KeyCode::Down => session.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => session.move_up(),
                    KeyCode::Enter => session.quarter_page_down(usize::from(height)),
                    KeyCode::Char('d') if key.modifiers.is_empty() => {
                        session.page_down(usize::from(height))
                    }
                    KeyCode::Char('u') if key.modifiers.is_empty() => {
                        session.page_up(usize::from(height))
                    }
                    KeyCode::PageDown => session.page_down(usize::from(height)),
                    KeyCode::PageUp => session.page_up(usize::from(height)),
                    KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        session.move_to_bottom()
                    }
                    KeyCode::Char('g') => session.move_to_top(),
                    KeyCode::Char('G') => session.move_to_bottom(),
                    _ => {}
                }
            }
        }
    }

    /// Draws the current page to the terminal.
    fn render(
        &self,
        session: &mut InteractiveViewerSession,
        height: usize,
        width: usize,
        command_buffer: Option<&str>,
        status_message: Option<&str>,
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let frame = session.render_frame(height, width)?;
        let header = self.render_header_text(&frame, command_buffer, status_message);
        execute!(
            stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(ClearType::All)
        )
        .map_err(|error| format!("failed to clear screen: {error}"))?;
        write!(stdout, "{header}")
            .map_err(|error| format!("failed to write header: {error}"))?;
        self.render_rows(stdout, &frame)?;
        stdout
            .flush()
            .map_err(|error| format!("failed to flush output: {error}"))?;
        Ok(())
    }

    /// Resolves the full header text, including transient command or status overlays.
    fn render_header_text(
        &self,
        frame: &RenderedFrame,
        command_buffer: Option<&str>,
        status_message: Option<&str>,
    ) -> String {
        if let Some(command_buffer) = command_buffer {
            return format!("{} | command={command_buffer}", frame.header);
        }
        if let Some(status_message) = status_message {
            return format!("{} | {status_message}", frame.header);
        }
        frame.header.clone()
    }

    /// Handles one key press while the viewer is in colon-command input mode.
    fn handle_command_key(
        &self,
        session: &mut InteractiveViewerSession,
        key_code: KeyCode,
        command_buffer: &mut Option<String>,
        status_message: &mut Option<String>,
    ) -> Result<(), String> {
        match key_code {
            KeyCode::Esc => {
                *command_buffer = None;
                *status_message = None;
            }
            KeyCode::Enter => {
                let command = command_buffer.take().unwrap_or_default();
                match self.execute_command(session, &command) {
                    Ok(()) => {
                        *status_message = None;
                    }
                    Err(error) => {
                        *status_message = Some(error);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(command) = command_buffer.as_mut() {
                    command.pop();
                    if command.is_empty() {
                        *command_buffer = None;
                    }
                }
            }
            KeyCode::Char(character) => {
                if let Some(command) = command_buffer.as_mut() {
                    command.push(character);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Executes one supported colon command against the active session.
    fn execute_command(
        &self,
        session: &mut InteractiveViewerSession,
        command: &str,
    ) -> Result<(), String> {
        let target_line = parse_line_jump_command(command)?;
        let target_index = u64::try_from(target_line.saturating_sub(1)).unwrap_or(u64::MAX);
        session.jump_to(target_index)
    }

    /// Draws all visible rows for the current frame.
    fn render_rows(&self, stdout: &mut io::Stdout, frame: &RenderedFrame) -> Result<(), String> {
        for (index, row) in frame.rows.iter().enumerate() {
            self.render_split_row(stdout, index + 1, frame, row)?;
        }
        Ok(())
    }

    /// Draws one split-pane row by writing the left and right areas independently.
    fn render_split_row(
        &self,
        stdout: &mut io::Stdout,
        row_index: usize,
        frame: &RenderedFrame,
        rendered_row: &RenderedRow,
    ) -> Result<(), String> {
        let row = u16::try_from(row_index)
            .map_err(|_| "terminal row index exceeds supported range".to_string())?;
        execute!(stdout, cursor::MoveTo(0, row))
            .map_err(|error| format!("failed to move cursor: {error}"))?;
        execute!(stdout, SetForegroundColor(LINE_NUMBER_COLOR))
            .map_err(|error| format!("failed to set line-number color: {error}"))?;
        write!(
            stdout,
            "{:>width$}",
            rendered_row.line_number.as_deref().unwrap_or(""),
            width = frame.line_number_width
        )
        .map_err(|error| format!("failed to write line-number pane: {error}"))?;
        execute!(stdout, ResetColor).map_err(|error| format!("failed to reset color: {error}"))?;
        execute!(stdout, cursor::MoveTo(frame.line_number_width as u16, row))
            .map_err(|error| format!("failed to move to current-row marker: {error}"))?;
        write!(stdout, "{FOCUS_MARKER_OFFSET}")
            .map_err(|error| format!("failed to write current-row marker offset: {error}"))?;
        if rendered_row.focus == RowFocus::Focused && rendered_row.line_number.is_some() {
            execute!(stdout, SetForegroundColor(CURRENT_MARKER_COLOR))
                .map_err(|error| format!("failed to set current-row marker color: {error}"))?;
            write!(stdout, "{CURRENT_ROW_MARKER}")
                .map_err(|error| format!("failed to write current-row marker: {error}"))?;
            execute!(stdout, ResetColor)
                .map_err(|error| format!("failed to reset color: {error}"))?;
        } else {
            write!(stdout, "{INACTIVE_ROW_MARKER}")
                .map_err(|error| format!("failed to clear current-row marker: {error}"))?;
        }
        execute!(
            stdout,
            cursor::MoveTo((frame.line_number_width + 2) as u16, row)
        )
        .map_err(|error| format!("failed to move to content pane: {error}"))?;
        write!(stdout, "{CONTENT_PADDING}")
            .map_err(|error| format!("failed to write content padding: {error}"))?;
        execute!(
            stdout,
            cursor::MoveTo((frame.line_number_width + 2) as u16, row)
        )
        .map_err(|error| format!("failed to move to padded content pane: {error}"))?;
        write!(
            stdout,
            "{:<width$}",
            rendered_row.content,
            width = frame.content_width
        )
        .map_err(|error| format!("failed to write content pane: {error}"))?;
        Ok(())
    }
}

/// Parses one `:<lineNumber>` command into a 1-based logical line number.
fn parse_line_jump_command(command: &str) -> Result<usize, String> {
    let line_number_text = command
        .strip_prefix(LINE_JUMP_COMMAND_PREFIX)
        .ok_or_else(|| "unsupported command, use :<lineNumber>".to_string())?;
    if line_number_text.is_empty() {
        return Err("missing line number after :".to_string());
    }
    let line_number = line_number_text
        .parse::<usize>()
        .map_err(|error| format!("invalid line number '{line_number_text}': {error}"))?;
    if line_number == 0 {
        return Err("line number must be greater than zero".to_string());
    }
    Ok(line_number)
}

#[cfg(test)]
mod tests {
    use super::{parse_line_jump_command, ViewerApplication};
    use crate::config::ViewerConfig;

    #[test]
    fn banner_contains_target_file_when_provided() {
        let mut config = ViewerConfig::default();
        config.log_file = Some("demo.tog".to_string());

        let app = ViewerApplication::new(config);

        assert_eq!(
            app.banner(),
            "Tinylog Viewer scaffold initialized for demo.tog."
        );
    }

    #[test]
    fn parse_line_jump_command_accepts_colon_number() {
        assert_eq!(parse_line_jump_command(":128").expect("parse jump command"), 128);
    }

    #[test]
    fn parse_line_jump_command_rejects_invalid_inputs() {
        assert_eq!(
            parse_line_jump_command("+42").expect_err("reject unsupported command"),
            "unsupported command, use :<lineNumber>"
        );
        assert_eq!(
            parse_line_jump_command(":0").expect_err("reject zero line"),
            "line number must be greater than zero"
        );
    }
}
