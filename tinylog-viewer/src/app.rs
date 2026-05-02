use crate::{
    config::ViewerConfig,
    session::{InteractiveViewerSession, RenderedFrame, RenderedRow, RowFocus},
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, Write};

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
            Some(path) => format!("tinylog viewer scaffold initialized for {path}."),
            None => "tinylog viewer scaffold initialized.".to_string(),
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
        loop {
            let (width, height) = terminal::size()
                .map_err(|error| format!("failed to query terminal size: {error}"))?;
            self.render(session, usize::from(height), usize::from(width), stdout)?;
            let event =
                event::read().map_err(|error| format!("failed to read key event: {error}"))?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
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
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let frame = session.render_frame(height, width)?;
        execute!(
            stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(ClearType::All)
        )
        .map_err(|error| format!("failed to clear screen: {error}"))?;
        write!(stdout, "{}", frame.header)
            .map_err(|error| format!("failed to write header: {error}"))?;
        self.render_rows(stdout, &frame)?;
        stdout
            .flush()
            .map_err(|error| format!("failed to flush output: {error}"))?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::ViewerApplication;
    use crate::config::ViewerConfig;

    #[test]
    fn banner_contains_target_file_when_provided() {
        let mut config = ViewerConfig::default();
        config.log_file = Some("demo.tog".to_string());

        let app = ViewerApplication::new(config);

        assert_eq!(
            app.banner(),
            "tinylog viewer scaffold initialized for demo.tog."
        );
    }
}
