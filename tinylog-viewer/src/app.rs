use crate::{
    config::ViewerConfig,
    format,
    session::{
        InteractiveViewerSession, RenderedFrame, RenderedRow, RowFocus, SearchProgressAction, ViewerSession,
    },
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use std::time::Duration;

const LINE_JUMP_COMMAND_PREFIX: &str = ":";
const SEARCH_COMMAND_PREFIX: &str = "/";
const HELP_COMMAND: &str = ":help";
const LINE_NUMBER_COLOR: Color = Color::Blue;
const CURRENT_MARKER_COLOR: Color = Color::Rgb {
    r: 255,
    g: 196,
    b: 128,
};
const SEARCH_HIGHLIGHT_BACKGROUND_COLOR: Color = Color::Yellow;
const FOCUS_MARKER_OFFSET: &str = " ";
const CURRENT_ROW_MARKER: &str = "▪";
const INACTIVE_ROW_MARKER: &str = " ";
const CONTENT_PADDING: &str = "";
const HELP_POPUP_LINES: &[&str] = &[
    "Tinylog Viewer Help",
    "",
    "j / DownArrow   move down",
    "k / UpArrow     move up",
    "Enter           move down by 1/4 page",
    "d / PageDown    page down",
    "u / PageUp      page up",
    "g               jump to top",
    "G               jump to bottom",
    ":N              jump to line N",
    "/keyword        search keyword",
    ":debug          filter one level (also :trace/:info/:warn/:error)",
    "n               next search result / continue search",
    "p               previous search result / continue search",
    "Esc             clear filter/search or close help",
    "q               quit",
];

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
        let mut help_popup_visible = false;
        loop {
            let (width, height) = terminal::size()
                .map_err(|error| format!("failed to query terminal size: {error}"))?;
            let width_usize = usize::from(width);
            let height_usize = usize::from(height);
            self.render(
                session,
                height_usize,
                width_usize,
                command_buffer.as_deref(),
                status_message.as_deref(),
                help_popup_visible,
                stdout,
            )?;
            let event =
                event::read().map_err(|error| format!("failed to read key event: {error}"))?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if help_popup_visible {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            help_popup_visible = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                if command_buffer.is_some() {
                    self.handle_command_key(
                        session,
                        key.code,
                        height_usize,
                        width_usize,
                        stdout,
                        &mut command_buffer,
                        &mut status_message,
                        &mut help_popup_visible,
                    )?;
                    continue;
                }
                if key.code == KeyCode::Esc {
                    if session.has_active_filter() {
                        session.clear_filter();
                        status_message = Some("filter cleared".to_string());
                        continue;
                    }
                    if session.has_active_search() {
                        session.clear_search();
                        status_message = Some("search cleared".to_string());
                        continue;
                    }
                }
                status_message = None;
                match key.code {
                    KeyCode::Char('q')
                        if !session.has_active_filter() && !session.has_active_search() =>
                    {
                        return Ok(());
                    }
                    KeyCode::Char(':') => {
                        command_buffer = Some(":".to_string());
                    }
                    KeyCode::Char('/') => {
                        command_buffer = Some("/".to_string());
                    }
                    KeyCode::Char('j') | KeyCode::Down => session.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => session.move_up(),
                    KeyCode::Char('n') => {
                        if let Err(error) = session.move_to_next_search_result_with_progress(|search_session, progress| {
                            self.handle_search_progress(
                                search_session,
                                search_session.active_search_keyword().unwrap_or_default(),
                                progress,
                                width_usize,
                                stdout,
                                &mut status_message,
                            )
                        }) {
                            status_message = Some(error);
                        }
                    }
                    KeyCode::Char('p') => {
                        if let Err(error) = session.move_to_previous_search_result_with_progress(
                            |search_session, progress| {
                                self.handle_search_progress(
                                    search_session,
                                    search_session.active_search_keyword().unwrap_or_default(),
                                    progress,
                                    width_usize,
                                    stdout,
                                    &mut status_message,
                                )
                            },
                        ) {
                            status_message = Some(error);
                        }
                    }
                    KeyCode::Enter => session.quarter_page_down(height_usize),
                    KeyCode::Char('d') if key.modifiers.is_empty() => {
                        session.page_down(height_usize)
                    }
                    KeyCode::Char('u') if key.modifiers.is_empty() => {
                        session.page_up(height_usize)
                    }
                    KeyCode::PageDown => session.page_down(height_usize),
                    KeyCode::PageUp => session.page_up(height_usize),
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
        help_popup_visible: bool,
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let frame = session.render_frame(height, width)?;
        let header = self.render_header_text(&frame, command_buffer, status_message);
        self.render_header_line(stdout, &header, width)?;
        self.render_rows(stdout, &frame)?;
        self.clear_remaining_rows(stdout, frame.rows.len().saturating_add(1), height)?;
        if help_popup_visible {
            self.render_help_popup(stdout, width, height)?;
        }
        stdout
            .flush()
            .map_err(|error| format!("failed to flush output: {error}"))?;
        Ok(())
    }

    /// Updates only the header while a search is still scanning trunks.
    fn render_search_progress(
        &self,
        session: &InteractiveViewerSession,
        width: usize,
        status_message: &str,
        stdout: &mut io::Stdout,
    ) -> Result<(), String> {
        let header = format!("{} | {status_message}", session.header_text());
        self.render_header_line(stdout, &header, width)?;
        stdout
            .flush()
            .map_err(|error| format!("failed to flush search progress: {error}"))?;
        Ok(())
    }

    /// Updates search progress and allows Esc to cancel the in-flight scan.
    fn handle_search_progress(
        &self,
        session: &InteractiveViewerSession,
        keyword: &str,
        progress: crate::session::SearchProgress,
        width: usize,
        stdout: &mut io::Stdout,
        status_message: &mut Option<String>,
    ) -> Result<SearchProgressAction, String> {
        *status_message = Some(format!(
            "searching /{keyword} | {}% | trunk={}/{}",
            progress.percentage(),
            progress.current_trunk_position,
            progress.total_trunks
        ));
        self.render_search_progress(
            session,
            width,
            status_message.as_deref().unwrap_or_default(),
            stdout,
        )?;
        if event::poll(Duration::from_millis(0))
            .map_err(|error| format!("failed to poll cancel key: {error}"))?
        {
            let pending_event =
                event::read().map_err(|error| format!("failed to read cancel key: {error}"))?;
            if let Event::Key(key) = pending_event {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    *status_message = Some("search canceled".to_string());
                    return Ok(SearchProgressAction::Cancel);
                }
            }
        }
        Ok(SearchProgressAction::Continue)
    }

    /// Updates filter progress and allows Esc to cancel the in-flight scan.
    fn handle_filter_progress(
        &self,
        session: &InteractiveViewerSession,
        level_name: &str,
        progress: crate::session::SearchProgress,
        width: usize,
        stdout: &mut io::Stdout,
        status_message: &mut Option<String>,
    ) -> Result<SearchProgressAction, String> {
        *status_message = Some(format!(
            "filtering :{level_name} | {}% | trunk={}/{}",
            progress.percentage(),
            progress.current_trunk_position,
            progress.total_trunks
        ));
        self.render_search_progress(
            session,
            width,
            status_message.as_deref().unwrap_or_default(),
            stdout,
        )?;
        if event::poll(Duration::from_millis(0))
            .map_err(|error| format!("failed to poll cancel key: {error}"))?
        {
            let pending_event =
                event::read().map_err(|error| format!("failed to read cancel key: {error}"))?;
            if let Event::Key(key) = pending_event {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    *status_message = Some("filter canceled".to_string());
                    return Ok(SearchProgressAction::Cancel);
                }
            }
        }
        Ok(SearchProgressAction::Continue)
    }

    /// Writes the first terminal row and pads or clears any previous content.
    fn render_header_line(
        &self,
        stdout: &mut io::Stdout,
        header: &str,
        width: usize,
    ) -> Result<(), String> {
        execute!(
            stdout,
            cursor::MoveTo(0, 0),
            terminal::Clear(ClearType::CurrentLine)
        )
        .map_err(|error| format!("failed to prepare header line: {error}"))?;
        let header_char_count = header.chars().count();
        if header_char_count >= width {
            let clipped_header: String = header.chars().take(width).collect();
            write!(stdout, "{clipped_header}")
                .map_err(|error| format!("failed to write header: {error}"))?;
        } else {
            write!(stdout, "{header}{:padding$}", "", padding = width - header_char_count)
                .map_err(|error| format!("failed to write header: {error}"))?;
        }
        Ok(())
    }

    /// Clears rows below the rendered viewport when terminal height changes.
    fn clear_remaining_rows(
        &self,
        stdout: &mut io::Stdout,
        start_row_index: usize,
        height: usize,
    ) -> Result<(), String> {
        for row_index in start_row_index..height {
            let row = u16::try_from(row_index)
                .map_err(|_| "terminal row index exceeds supported range".to_string())?;
            execute!(
                stdout,
                cursor::MoveTo(0, row),
                terminal::Clear(ClearType::CurrentLine)
            )
            .map_err(|error| format!("failed to clear stale row: {error}"))?;
        }
        Ok(())
    }

    /// Draws a centered help popup on top of the current page.
    fn render_help_popup(&self, stdout: &mut io::Stdout, width: usize, height: usize) -> Result<(), String> {
        let popup_width = HELP_POPUP_LINES
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
            .saturating_add(4)
            .min(width.max(4));
        let popup_height = HELP_POPUP_LINES.len().saturating_add(2).min(height.max(2));
        let start_col = width.saturating_sub(popup_width) / 2;
        let start_row = height.saturating_sub(popup_height) / 2;
        let inner_width = popup_width.saturating_sub(2);
        let top_bottom = format!("+{}+", "-".repeat(inner_width));

        for row_offset in 0..popup_height {
            let row = u16::try_from(start_row.saturating_add(row_offset))
                .map_err(|_| "terminal row index exceeds supported range".to_string())?;
            execute!(
                stdout,
                cursor::MoveTo(u16::try_from(start_col).unwrap_or(0), row),
                terminal::Clear(ClearType::UntilNewLine)
            )
            .map_err(|error| format!("failed to draw help popup: {error}"))?;
            let line = if row_offset == 0 || row_offset == popup_height.saturating_sub(1) {
                top_bottom.clone()
            } else {
                let content = HELP_POPUP_LINES
                    .get(row_offset.saturating_sub(1))
                    .copied()
                    .unwrap_or("");
                let clipped: String = content.chars().take(inner_width).collect();
                let padding = inner_width.saturating_sub(clipped.chars().count());
                format!("|{}{}|", clipped, " ".repeat(padding))
            };
            write!(stdout, "{line}").map_err(|error| format!("failed to write help popup: {error}"))?;
        }
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
        height: usize,
        width: usize,
        stdout: &mut io::Stdout,
        command_buffer: &mut Option<String>,
        status_message: &mut Option<String>,
        help_popup_visible: &mut bool,
    ) -> Result<(), String> {
        match key_code {
            KeyCode::Esc => {
                *command_buffer = None;
                *status_message = None;
            }
            KeyCode::Enter => {
                let command = command_buffer.take().unwrap_or_default();
                match self.execute_command(
                    session,
                    &command,
                    height,
                    width,
                    stdout,
                    status_message,
                    help_popup_visible,
                ) {
                    Ok(()) => {
                        if status_message.is_none() {
                            *status_message = None;
                        }
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
        height: usize,
        width: usize,
        stdout: &mut io::Stdout,
        status_message: &mut Option<String>,
        help_popup_visible: &mut bool,
    ) -> Result<(), String> {
        if command.trim() == HELP_COMMAND {
            *help_popup_visible = true;
            *status_message = None;
            return Ok(());
        }
        if command.starts_with(LINE_JUMP_COMMAND_PREFIX) {
            let command_body = command.trim_start_matches(LINE_JUMP_COMMAND_PREFIX).trim();
            if command_body.chars().all(|character| character.is_ascii_digit()) {
                let target_line = parse_line_jump_command(command)?;
                let target_index = u64::try_from(target_line.saturating_sub(1)).unwrap_or(u64::MAX);
                session.jump_to(target_index)?;
                *status_message = None;
                return Ok(());
            }
            let level = parse_level_filter_command(command)?;
            session.apply_level_filter_with_progress(level, height, width, |filter_session, progress| {
                self.handle_filter_progress(
                    filter_session,
                    level.command_name(),
                    progress,
                    width,
                    stdout,
                    status_message,
                )
            })?;
            *status_message = Some(format!("filter=:{}", level.command_name()));
            return Ok(());
        }
        if command.starts_with(SEARCH_COMMAND_PREFIX) {
            let keyword = parse_search_command(command)?;
            let summary = session.search_with_progress(keyword, |search_session, progress| {
                self.handle_search_progress(search_session, keyword, progress, width, stdout, status_message)
            })?;
            *status_message = Some(format!(
                "search=/{}, results={}",
                summary.keyword, summary.total_matches
            ));
            return Ok(());
        }
        Err("unsupported command, use :<lineNumber>, :help, :<level>, or /<keyword>".to_string())
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
        execute!(
            stdout,
            cursor::MoveTo(0, row),
            terminal::Clear(ClearType::CurrentLine)
        )
        .map_err(|error| format!("failed to prepare row: {error}"))?;
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
        self.write_content_with_highlights(stdout, &rendered_row.content, &rendered_row.highlight_ranges, frame.content_width)?;
        Ok(())
    }

    /// Writes one content pane row and colors search hits in yellow.
    fn write_content_with_highlights(
        &self,
        stdout: &mut io::Stdout,
        content: &str,
        highlight_ranges: &[(usize, usize)],
        width: usize,
    ) -> Result<(), String> {
        let characters: Vec<char> = content.chars().collect();
        let mut cursor_index = 0usize;
        for (highlight_start, highlight_end) in highlight_ranges {
            if *highlight_start > cursor_index {
                let plain_text: String = characters[cursor_index..*highlight_start].iter().collect();
                write!(stdout, "{plain_text}")
                    .map_err(|error| format!("failed to write content pane: {error}"))?;
            }
            if *highlight_end > *highlight_start {
                execute!(stdout, SetBackgroundColor(SEARCH_HIGHLIGHT_BACKGROUND_COLOR))
                    .map_err(|error| format!("failed to set search highlight background: {error}"))?;
                let highlighted_text: String = characters[*highlight_start..*highlight_end].iter().collect();
                write!(stdout, "{highlighted_text}")
                    .map_err(|error| format!("failed to write highlighted content: {error}"))?;
                execute!(stdout, ResetColor)
                    .map_err(|error| format!("failed to reset search highlight style: {error}"))?;
            }
            cursor_index = *highlight_end;
        }
        if cursor_index < characters.len() {
            let remaining_text: String = characters[cursor_index..].iter().collect();
            write!(stdout, "{remaining_text}")
                .map_err(|error| format!("failed to write content pane remainder: {error}"))?;
        }
        if characters.len() < width {
            write!(stdout, "{:width$}", "", width = width - characters.len())
                .map_err(|error| format!("failed to pad content pane: {error}"))?;
        }
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

/// Parses one `/keyword` command into the user-entered search term.
fn parse_search_command(command: &str) -> Result<&str, String> {
    let keyword = command
        .strip_prefix(SEARCH_COMMAND_PREFIX)
        .ok_or_else(|| "unsupported command, use /<keyword>".to_string())?
        .trim();
    if keyword.is_empty() {
        return Err("missing keyword after /".to_string());
    }
    Ok(keyword)
}

/// Parses one `:level` command into a structured log level.
fn parse_level_filter_command(command: &str) -> Result<format::LogLevel, String> {
    let level_text = command
        .strip_prefix(LINE_JUMP_COMMAND_PREFIX)
        .ok_or_else(|| "unsupported command, use :<level>".to_string())?
        .trim();
    if level_text.is_empty() {
        return Err("missing level after :".to_string());
    }
    format::LogLevel::parse_token(level_text)
        .ok_or_else(|| format!("unsupported level '{level_text}', use trace/debug/info/warn/error"))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_level_filter_command, parse_line_jump_command, parse_search_command, ViewerApplication,
    };
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

    #[test]
    fn parse_search_command_accepts_slash_keyword() {
        assert_eq!(parse_search_command("/alpha").expect("parse search command"), "alpha");
    }

    #[test]
    fn parse_search_command_rejects_empty_keyword() {
        assert_eq!(
            parse_search_command("/").expect_err("reject empty keyword"),
            "missing keyword after /"
        );
    }

    #[test]
    fn parse_level_filter_command_accepts_colon_level() {
        assert_eq!(
            parse_level_filter_command(":debug")
                .expect("parse level filter")
                .command_name(),
            "debug"
        );
    }

    #[test]
    fn parse_level_filter_command_rejects_unknown_level() {
        assert_eq!(
            parse_level_filter_command(":verbose").expect_err("reject unknown level"),
            "unsupported level 'verbose', use trace/debug/info/warn/error"
        );
    }
}
