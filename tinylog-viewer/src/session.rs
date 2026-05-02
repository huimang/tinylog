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

    /// Renders the current visible window into terminal lines.
    pub fn render_lines(&self, height: usize) -> Result<Vec<String>, String> {
        let visible_count = self.visible_count(height);
        let window = format::read_visible_window(&self.log_file, self.top_index, visible_count)?;
        let line_number_width = self.total_records.to_string().len().max(1);
        let mut lines = Vec::new();
        lines.push(format!(
            "tinylog viewer | file={} | records={} | line={} | j/k move  enter +1/4  d/u page  g/G ends  q quit",
            self.log_file,
            window.total_records,
            self.top_index.saturating_add(1)
        ));
        lines.push(String::new());
        for (index, entry) in window.visible_entries.into_iter().enumerate() {
            lines.push(format!(
                "{:>width$} {} {}",
                self.top_index.saturating_add(index).saturating_add(1),
                format::format_timestamp_millis(entry.timestamp_millis)?,
                entry.content,
                width = line_number_width
            ));
        }
        let remaining = self.visible_count(height).saturating_sub(lines.len().saturating_sub(2));
        for _ in 0..remaining {
            lines.push(String::new());
        }
        Ok(lines)
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
        let page = self.visible_count(height).max(1);
        let max_top = self.total_records.saturating_sub(1);
        self.top_index = usize::min(self.top_index.saturating_add(page), max_top);
    }

    /// Moves the window up by one page.
    pub fn page_up(&mut self, height: usize) {
        let page = self.visible_count(height).max(1);
        self.top_index = self.top_index.saturating_sub(page);
    }

    /// Moves the window down by one quarter of the current page.
    pub fn quarter_page_down(&mut self, height: usize) {
        let step = (self.visible_count(height).max(1) / 4).max(1);
        let max_top = self.total_records.saturating_sub(1);
        self.top_index = usize::min(self.top_index.saturating_add(step), max_top);
    }

    /// Moves the window to the first record.
    pub fn move_to_top(&mut self) {
        self.top_index = 0;
    }

    /// Moves the window to the last renderable page.
    pub fn move_to_bottom(&mut self, height: usize) {
        let page = self.visible_count(height).max(1);
        self.top_index = self.total_records.saturating_sub(page);
    }

    /// Returns the visible page size derived from terminal height and configuration.
    fn visible_count(&self, height: usize) -> usize {
        let terminal_page = height.saturating_sub(2).max(1);
        usize::min(self.preferred_page_size, terminal_page).max(1)
    }
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
        let first_page = session.render_lines(4).expect("render first page");
        session.move_down();
        let second_page = session.render_lines(4).expect("render second page");

        assert!(first_page.iter().any(|line| line.contains("1 2026-05-01 22:01:00,253 alpha")));
        assert!(first_page.iter().any(|line| line.contains("2 2026-05-01 22:01:00,278 beta")));
        assert!(!first_page.iter().any(|line| line.contains("gamma")));
        assert!(second_page.iter().any(|line| line.contains("2 2026-05-01 22:01:00,278 beta")));
        assert!(second_page.iter().any(|line| line.contains("3 2026-05-01 22:01:00,303 gamma")));

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
}
