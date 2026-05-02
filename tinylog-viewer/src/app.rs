use crate::config::ViewerConfig;
use crate::session::InteractiveViewerSession;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, ClearType};
use std::io::{self, Write};

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
        let log_file = match self.config.log_file.clone() {
            Some(path) => path,
            None => {
                println!("{}", self.banner());
                return Ok(());
            }
        };

        let mut session = InteractiveViewerSession::open(log_file, self.config.page_size)?;
        let mut stdout = io::stdout();
        terminal::enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)
            .map_err(|error| format!("failed to enter alternate screen: {error}"))?;

        let result = self.run_loop(&mut session, &mut stdout);

        let cleanup_result = execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)
            .map_err(|error| format!("failed to leave alternate screen: {error}"));
        let raw_mode_result =
            terminal::disable_raw_mode().map_err(|error| format!("failed to disable raw mode: {error}"));

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
            let (_, height) =
                terminal::size().map_err(|error| format!("failed to query terminal size: {error}"))?;
            self.render(session, usize::from(height), stdout)?;
            let event = event::read().map_err(|error| format!("failed to read key event: {error}"))?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => session.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => session.move_up(),
                    KeyCode::Enter => session.quarter_page_down(usize::from(height)),
                    KeyCode::Char('d') if key.modifiers.is_empty() => session.page_down(usize::from(height)),
                    KeyCode::Char('u') if key.modifiers.is_empty() => session.page_up(usize::from(height)),
                    KeyCode::PageDown => session.page_down(usize::from(height)),
                    KeyCode::PageUp => session.page_up(usize::from(height)),
                    KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        session.move_to_bottom(usize::from(height))
                    }
                    KeyCode::Char('g') => session.move_to_top(),
                    KeyCode::Char('G') => session.move_to_bottom(usize::from(height)),
                    _ => {}
                }
            }
        }
    }

    /// Draws the current page to the terminal.
    fn render(
        &self,
        session: &InteractiveViewerSession,
        height: usize,
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let lines = session.render_lines(height)?;
        execute!(stdout, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All))
            .map_err(|error| format!("failed to clear screen: {error}"))?;
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                writeln!(stdout).map_err(|error| format!("failed to write newline: {error}"))?;
            }
            write!(stdout, "{line}").map_err(|error| format!("failed to write line: {error}"))?;
        }
        stdout.flush().map_err(|error| format!("failed to flush output: {error}"))?;
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
