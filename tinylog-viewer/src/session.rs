use crate::format;

#[allow(dead_code)]
/// Defines the business actions expected from an interactive log browsing session.
pub trait ViewerSession {
    /// Returns the current target file, if one has been opened.
    fn log_file(&self) -> Option<&str>;

    /// Starts a forward browsing workflow.
    fn browse(&mut self) -> Result<(), String>;

    /// Searches the current log source by a business keyword.
    fn search(&mut self, keyword: &str) -> Result<(), String>;

    /// Moves the session to a logical byte offset or indexed position.
    fn jump_to(&mut self, offset: u64) -> Result<(), String>;
}

/// Stores the state required to render and navigate one visible log window.
#[derive(Debug, Clone)]
pub struct InteractiveViewerSession {
    log_file: String,
    top_index: usize,
    total_records: usize,
    preferred_page_size: usize,
}

/// Holds one rendered viewer frame with pane metadata and visible rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFrame {
    pub header: String,
    pub line_number_width: usize,
    pub rows: Vec<RenderedRow>,
}

/// Holds one rendered row split into left and right pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedRow {
    pub line_number: Option<String>,
    pub content: String,
}

impl InteractiveViewerSession {
    /// Opens a session for one target log file.
    pub fn open(log_file: String, preferred_page_size: usize) -> Result<Self, String> {
        let header = format::read_visible_window(&log_file, 0, 0)?;
        Ok(Self {
            log_file,
            top_index: 0,
            total_records: usize::try_from(header.total_records).unwrap_or(usize::MAX),
            preferred_page_size,
        })
    }

    /// Returns the current top index of the visible window.
    #[cfg(test)]
    pub fn top_index(&self) -> usize {
        self.top_index
    }

    /// Returns the total record count known from the file header.
    #[cfg(test)]
    pub fn total_records(&self) -> usize {
        self.total_records
    }

    /// Renders the current visible window into a split-pane frame.
    pub fn render_frame(&self, height: usize, width: usize) -> Result<RenderedFrame, String> {
        let visible_count = self.visible_record_count(height);
        let window = format::read_visible_window(&self.log_file, self.top_index, visible_count)?;
        let line_number_width = self.total_records.to_string().len().max(6);
        let content_width = width.saturating_sub(line_number_width + 3).max(1);
        let visible_row_capacity = height.saturating_sub(2);
        let mut rows = Vec::new();
        let header = format!(
            "tinylog viewer | file={} | records={} | line={} | j/k move  enter +1/4  d/u page  g/G ends  q quit",
            self.log_file,
            window.total_records,
            self.top_index.saturating_add(1)
        );
        for (index, entry) in window.visible_entries.into_iter().enumerate() {
            let logical_line_number = self.top_index.saturating_add(index).saturating_add(1);
            let rendered_content = wrap_content(
                &format!(
                    "{} {}",
                    format::format_timestamp_millis(entry.timestamp_millis)?,
                    entry.content
                ),
                content_width,
            );
            for (rendered_index, rendered_line) in rendered_content.into_iter().enumerate() {
                rows.push(RenderedRow {
                    line_number: if rendered_index == 0 {
                        Some(logical_line_number.to_string())
                    } else {
                        None
                    },
                    content: rendered_line,
                });
                if rows.len() >= visible_row_capacity {
                    break;
                }
            }
            if rows.len() >= visible_row_capacity {
                break;
            }
        }
        let remaining = visible_row_capacity.saturating_sub(rows.len());
        for _ in 0..remaining {
            rows.push(RenderedRow {
                line_number: None,
                content: String::new(),
            });
        }
        Ok(RenderedFrame {
            header,
            line_number_width,
            rows,
        })
    }

    /// Moves the window down by one record.
    pub fn move_down(&mut self) {
        if self.top_index.saturating_add(1) < self.total_records {
            self.top_index = self.top_index.saturating_add(1);
        }
    }

    /// Moves the window up by one record.
    pub fn move_up(&mut self) {
        self.top_index = self.top_index.saturating_sub(1);
    }

    /// Moves the window down by one page.
    pub fn page_down(&mut self, height: usize) {
        let page = self.visible_record_count(height).max(1);
        let max_top = self.total_records.saturating_sub(1);
        self.top_index = usize::min(self.top_index.saturating_add(page), max_top);
    }

    /// Moves the window up by one page.
    pub fn page_up(&mut self, height: usize) {
        let page = self.visible_record_count(height).max(1);
        self.top_index = self.top_index.saturating_sub(page);
    }

    /// Moves the window down by one quarter of the current page.
    pub fn quarter_page_down(&mut self, height: usize) {
        let step = (self.visible_record_count(height).max(1) / 4).max(1);
        let max_top = self.total_records.saturating_sub(1);
        self.top_index = usize::min(self.top_index.saturating_add(step), max_top);
    }

    /// Moves the window to the first record.
    pub fn move_to_top(&mut self) {
        self.top_index = 0;
    }

    /// Moves the window to the last renderable page.
    pub fn move_to_bottom(&mut self, height: usize) {
        let page = self.visible_record_count(height).max(1);
        self.top_index = self.total_records.saturating_sub(page);
    }

    /// Returns the visible record count derived from terminal height and configuration.
    fn visible_record_count(&self, height: usize) -> usize {
        let terminal_page = height.saturating_sub(2).max(1);
        usize::min(self.preferred_page_size, terminal_page).max(1)
    }
}

/// Wraps one logical log line into the right-hand content area while preserving explicit newlines.
fn wrap_content(content: &str, content_width: usize) -> Vec<String> {
    let mut rendered_lines = Vec::new();
    for segment in content.split('\n') {
        if segment.is_empty() {
            rendered_lines.push(String::new());
            continue;
        }
        let characters: Vec<char> = segment.chars().collect();
        let mut start = 0usize;
        while start < characters.len() {
            let end = usize::min(start.saturating_add(content_width), characters.len());
            rendered_lines.push(characters[start..end].iter().collect());
            start = end;
        }
    }
    if rendered_lines.is_empty() {
        rendered_lines.push(String::new());
    }
    rendered_lines
}

impl ViewerSession for InteractiveViewerSession {
    fn log_file(&self) -> Option<&str> {
        Some(&self.log_file)
    }

    fn browse(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn search(&mut self, _keyword: &str) -> Result<(), String> {
        Err("search is not implemented yet".to_string())
    }

    fn jump_to(&mut self, offset: u64) -> Result<(), String> {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        self.top_index = usize::min(offset, self.total_records.saturating_sub(1));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::InteractiveViewerSession;

    /// Builds one valid three-record prototype file for session tests.
    fn sample_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&512_u16.to_be_bytes());
        bytes.extend_from_slice(&1_777_672_860_253_u64.to_be_bytes());
        bytes.extend_from_slice(&3_u64.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&3_u16.to_be_bytes());
        let mut trunk = Vec::new();
        trunk.extend_from_slice(&0_u32.to_be_bytes());
        trunk.extend_from_slice(&5_u32.to_be_bytes());
        trunk.extend_from_slice(b"alpha");
        trunk.extend_from_slice(&25_u32.to_be_bytes());
        trunk.extend_from_slice(&4_u32.to_be_bytes());
        trunk.extend_from_slice(b"beta");
        trunk.extend_from_slice(&50_u32.to_be_bytes());
        trunk.extend_from_slice(&5_u32.to_be_bytes());
        trunk.extend_from_slice(b"gamma");
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        bytes
    }

    #[test]
    fn render_lines_respects_current_window() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-test-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(&path, sample_bytes()).expect("write prototype file");

        let mut session =
            InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 2).expect("open session");
        let first_page = session.render_frame(4, 80).expect("render first page");
        session.move_down();
        let second_page = session.render_frame(4, 80).expect("render second page");

        assert_eq!(first_page.rows[0].line_number.as_deref(), Some("1"));
        assert!(first_page.rows[0].content.contains("2026-05-01 22:01:00,253 alpha"));
        assert_eq!(first_page.rows[1].line_number.as_deref(), Some("2"));
        assert!(first_page.rows[1].content.contains("2026-05-01 22:01:00,278 beta"));
        assert!(!first_page.rows.iter().any(|row| row.content.contains("gamma")));
        assert_eq!(second_page.rows[0].line_number.as_deref(), Some("2"));
        assert_eq!(second_page.rows[1].line_number.as_deref(), Some("3"));
        assert!(second_page.rows[1].content.contains("2026-05-01 22:01:00,303 gamma"));

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn move_to_bottom_positions_last_page() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-bottom-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(&path, sample_bytes()).expect("write prototype file");

        let mut session =
            InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 2).expect("open session");
        session.move_to_bottom(4);

        assert_eq!(session.top_index(), 1);
        assert_eq!(session.total_records(), 3);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn quarter_page_down_moves_by_one_quarter_screen() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-quarter-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(&path, sample_bytes()).expect("write prototype file");

        let mut session =
            InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 8).expect("open session");
        session.quarter_page_down(10);

        assert_eq!(session.top_index(), 2);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn render_lines_wraps_content_in_right_display_area() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-wrap-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&512_u16.to_be_bytes());
        bytes.extend_from_slice(&1_777_672_860_253_u64.to_be_bytes());
        bytes.extend_from_slice(&1_u64.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        let message = b"alpha beta\ngamma delta";
        let mut trunk = Vec::new();
        trunk.extend_from_slice(&0_u32.to_be_bytes());
        trunk.extend_from_slice(&(message.len() as u32).to_be_bytes());
        trunk.extend_from_slice(message);
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        fs::write(&path, bytes).expect("write prototype file");

        let session =
            InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4).expect("open session");
        let rendered = session.render_frame(8, 24).expect("render wrapped page");

        assert_eq!(rendered.rows[0].line_number.as_deref(), Some("1"));
        assert!(rendered.rows[0].content.contains("2026-05-01 22:0"));
        assert_eq!(rendered.rows[1].line_number, None);
        assert!(!rendered.rows[1].content.is_empty());
        assert!(rendered
            .rows
            .iter()
            .any(|row| row.line_number.is_none() && row.content.contains("gamma delta")));

        fs::remove_file(path).expect("cleanup file");
    }
}
