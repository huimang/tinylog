use tinylog_rust_common::format;

const HEADER_HEIGHT: usize = 1;
const MIN_LINE_NUMBER_WIDTH: usize = 6;
const FOCUS_MARKER_OFFSET_WIDTH: usize = 1;
const FOCUS_MARKER_WIDTH: usize = 1;
const CONTENT_PADDING_WIDTH: usize = 0;
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
    file_index: format::TinylogFileIndex,
    top_index: usize,
    current_index: usize,
    total_records: usize,
    preferred_page_size: usize,
    active_search: Option<SearchState>,
    active_filter: Option<FilterState>,
}

/// Caches one completed keyword search together with per-trunk match lines.
#[derive(Debug, Clone)]
struct SearchState {
    keyword: String,
    #[cfg_attr(not(test), allow(dead_code))]
    trunk_matches: Vec<Vec<usize>>,
    ordered_matches: Vec<usize>,
    current_match_index: usize,
    trunk_order: Vec<usize>,
    next_trunk_cursor: usize,
    total_trunks: usize,
    origin_trunk_index: usize,
}

/// Keeps one active level filter for the current view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FilterState {
    level: format::LogLevel,
}

/// Reports trunk-based search progress to the interactive app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchProgress {
    pub scanned_trunks: usize,
    pub total_trunks: usize,
    pub current_trunk_position: usize,
}

impl SearchProgress {
    /// Returns the completed search percentage in whole numbers.
    pub fn percentage(&self) -> usize {
        if self.total_trunks == 0 {
            return 100;
        }
        self.scanned_trunks.saturating_mul(100) / self.total_trunks
    }
}

/// Describes whether a running search should continue or stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProgressAction {
    Continue,
    Cancel,
}

/// Describes the final outcome of one completed search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSummary {
    pub keyword: String,
    pub total_matches: usize,
}

/// Controls when a paused search stage should stop scanning more trunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchAdvanceMode {
    Initial,
    Next,
    Previous,
}

/// Holds one rendered viewer frame with pane metadata and visible rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFrame {
    pub header: String,
    pub line_number_width: usize,
    pub content_width: usize,
    pub rows: Vec<RenderedRow>,
}

/// Describes whether one rendered row owns the current focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowFocus {
    Focused,
    Unfocused,
}

/// Holds one rendered row split into left and right pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedRow {
    pub line_number: Option<String>,
    pub content: String,
    pub highlight_ranges: Vec<(usize, usize)>,
    pub focus: RowFocus,
}

impl InteractiveViewerSession {
    /// Opens a session for one target log file.
    pub fn open(log_file: String, preferred_page_size: usize) -> Result<Self, String> {
        let file_index = format::scan_file_index(&log_file)?;
        Ok(Self {
            log_file,
            total_records: usize::try_from(file_index.total_records()).unwrap_or(usize::MAX),
            file_index,
            top_index: 0,
            current_index: 0,
            preferred_page_size,
            active_search: None,
            active_filter: None,
        })
    }

    /// Returns the current top index of the visible window.
    #[cfg(test)]
    pub fn top_index(&self) -> usize {
        self.top_index
    }

    /// Returns the current focused record index.
    #[cfg(test)]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Returns the total record count known from the file header.
    #[cfg(test)]
    pub fn total_records(&self) -> usize {
        self.total_records
    }

    /// Returns cached search matches grouped by trunk for tests.
    #[cfg(test)]
    pub fn search_trunk_matches(&self) -> Option<&[Vec<usize>]> {
        self.active_search
            .as_ref()
            .map(|search_state| search_state.trunk_matches.as_slice())
    }

    /// Renders the current visible window into a split-pane frame.
    pub fn render_frame(&mut self, height: usize, width: usize) -> Result<RenderedFrame, String> {
        let line_number_width = self.line_number_width();
        let content_width = self.content_width(width, line_number_width);
        let visible_row_capacity = height.saturating_sub(HEADER_HEIGHT);
        let visible_record_count = self.visible_record_count(height);
        let rows = if self.active_filter.is_some() {
            self.collect_filtered_rows(content_width, visible_row_capacity)?
        } else if self.total_records > 0 && self.current_index == self.total_records.saturating_sub(1) {
            let (top_index, rows) =
                self.render_bottom_rows(content_width, visible_row_capacity, visible_record_count)?;
            self.top_index = top_index;
            rows
        } else {
            self.top_index =
                self.resolve_top_index(content_width, visible_row_capacity, visible_record_count)?;
            self.collect_rows(content_width, visible_row_capacity, visible_record_count)?
        };

        Ok(RenderedFrame {
            header: self.render_header(),
            line_number_width,
            content_width,
            rows,
        })
    }

    /// Returns the current header text without forcing a full frame render.
    pub(crate) fn header_text(&self) -> String {
        self.render_header()
    }

    /// Returns the active search keyword, if any.
    pub(crate) fn active_search_keyword(&self) -> Option<&str> {
        self.active_search.as_ref().map(|search_state| search_state.keyword.as_str())
    }

    /// Returns whether a search is currently active.
    pub(crate) fn has_active_search(&self) -> bool {
        self.active_search.is_some()
    }

    /// Returns whether a level filter is currently active.
    pub(crate) fn has_active_filter(&self) -> bool {
        self.active_filter.is_some()
    }

    /// Moves the window down by one record.
    pub fn move_down(&mut self) {
        if self.current_index.saturating_add(1) < self.total_records {
            self.current_index = self.current_index.saturating_add(1);
        }
    }

    /// Moves the window up by one record.
    pub fn move_up(&mut self) {
        self.current_index = self.current_index.saturating_sub(1);
    }

    /// Moves the window down by one page.
    pub fn page_down(&mut self, height: usize) {
        let page = self.visible_record_count(height).max(1);
        let max_index = self.total_records.saturating_sub(1);
        self.current_index = usize::min(self.current_index.saturating_add(page), max_index);
    }

    /// Moves the window up by one page.
    pub fn page_up(&mut self, height: usize) {
        let page = self.visible_record_count(height).max(1);
        self.current_index = self.current_index.saturating_sub(page);
    }

    /// Moves the window down by one quarter of the current page.
    pub fn quarter_page_down(&mut self, height: usize) {
        let step = (self.visible_record_count(height).max(1) / 4).max(1);
        let max_index = self.total_records.saturating_sub(1);
        self.current_index = usize::min(self.current_index.saturating_add(step), max_index);
    }

    /// Moves the window to the first record.
    pub fn move_to_top(&mut self) {
        self.current_index = 0;
        self.top_index = 0;
    }

    /// Moves the window to the last renderable page.
    pub fn move_to_bottom(&mut self) {
        self.current_index = self.total_records.saturating_sub(1);
    }

    /// Clears the current active search state and removes keyword highlighting.
    pub fn clear_search(&mut self) {
        self.active_search = None;
    }

    /// Enables one level filter for the current view.
    pub fn apply_level_filter(&mut self, level: format::LogLevel) {
        self.active_filter = Some(FilterState { level });
    }

    /// Enables one level filter and reports outward-scan progress until the current view is filled.
    pub fn apply_level_filter_with_progress<F>(
        &mut self,
        level: format::LogLevel,
        height: usize,
        width: usize,
        mut report_progress: F,
    ) -> Result<(), String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        let line_number_width = self.line_number_width();
        let content_width = self.content_width(width, line_number_width);
        let visible_row_capacity = height.saturating_sub(HEADER_HEIGHT);
        let current_trunk_index = self
            .file_index
            .trunk_index_for_record(self.current_index)
            .unwrap_or(0);
        let trunk_order = self.build_search_trunk_order(current_trunk_index);
        let mut rendered_row_count = 0usize;

        for (scanned_trunks, trunk_index) in trunk_order.iter().enumerate() {
            let entries = format::read_trunk_entries(&self.file_index, *trunk_index)?;
            let trunk_record_start = self
                .file_index
                .trunk_record_start(*trunk_index)
                .ok_or_else(|| format!("invalid trunk index: {trunk_index}"))?;
            for entry_offset in self.filter_entry_order(*trunk_index, trunk_record_start, entries.len()) {
                let entry = entries
                    .get(entry_offset)
                    .ok_or_else(|| "missing filtered trunk entry".to_string())?;
                if entry.level != level {
                    continue;
                }
                rendered_row_count = rendered_row_count
                    .saturating_add(self.rendered_content(entry, content_width)?.len());
                if rendered_row_count >= visible_row_capacity {
                    break;
                }
            }

            match report_progress(
                self,
                SearchProgress {
                    scanned_trunks: scanned_trunks.saturating_add(1),
                    total_trunks: trunk_order.len(),
                    current_trunk_position: trunk_index.saturating_add(1),
                },
            )? {
                SearchProgressAction::Continue => {}
                SearchProgressAction::Cancel => return Err("filter canceled".to_string()),
            }

            if rendered_row_count >= visible_row_capacity {
                break;
            }
        }

        self.apply_level_filter(level);
        Ok(())
    }

    /// Clears the current level filter.
    pub fn clear_filter(&mut self) {
        self.active_filter = None;
    }

    /// Moves to the next search result and scans more trunks on demand when needed.
    pub fn move_to_next_search_result_with_progress<F>(&mut self, mut report_progress: F) -> Result<(), String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        if self.active_search.is_none() {
            return Err("no active search".to_string());
        }

        if self.try_move_to_next_cached_search_result()? {
            return Ok(());
        }

        while self.search_has_pending_trunks() {
            self.advance_search_with_progress(SearchAdvanceMode::Next, &mut report_progress)?;
            if self.try_move_to_next_cached_search_result()? {
                return Ok(());
            }
        }

        let Some(search_state) = self.active_search.as_mut() else {
            return Err("no active search".to_string());
        };
        if search_state.ordered_matches.is_empty() {
            return Err(format!("keyword not found: {}", search_state.keyword));
        }
        search_state.current_match_index = 0;
        let target_index = search_state.ordered_matches[0];
        self.jump_to(u64::try_from(target_index).unwrap_or(u64::MAX))
    }

    /// Moves to the previous search result and scans more trunks on demand when needed.
    pub fn move_to_previous_search_result_with_progress<F>(
        &mut self,
        mut report_progress: F,
    ) -> Result<(), String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        if self.active_search.is_none() {
            return Err("no active search".to_string());
        }

        if self.try_move_to_previous_cached_search_result()? {
            return Ok(());
        }

        while self.search_has_pending_trunks() {
            self.advance_search_with_progress(SearchAdvanceMode::Previous, &mut report_progress)?;
            if self.try_move_to_previous_cached_search_result()? {
                return Ok(());
            }
        }

        let Some(search_state) = self.active_search.as_mut() else {
            return Err("no active search".to_string());
        };
        if search_state.ordered_matches.is_empty() {
            return Err(format!("keyword not found: {}", search_state.keyword));
        }
        search_state.current_match_index = search_state.ordered_matches.len().saturating_sub(1);
        let target_index = search_state.ordered_matches[search_state.current_match_index];
        self.jump_to(u64::try_from(target_index).unwrap_or(u64::MAX))
    }

    /// Returns the visible record count derived from terminal height and configuration.
    fn visible_record_count(&self, height: usize) -> usize {
        let terminal_page = height.saturating_sub(HEADER_HEIGHT).max(1);
        usize::min(self.preferred_page_size, terminal_page).max(1)
    }

    /// Returns the width needed for the line-number pane.
    fn line_number_width(&self) -> usize {
        self.total_records
            .to_string()
            .len()
            .max(MIN_LINE_NUMBER_WIDTH)
    }

    /// Returns the width available for content after the viewer gutter columns.
    fn content_width(&self, width: usize, line_number_width: usize) -> usize {
        width
            .saturating_sub(
                line_number_width
                    + FOCUS_MARKER_OFFSET_WIDTH
                    + FOCUS_MARKER_WIDTH
                    + CONTENT_PADDING_WIDTH,
            )
            .max(1)
    }

    /// Formats the header line for the current file and focused record.
    fn render_header(&self) -> String {
        let current_trunk = if self.total_records == 0 {
            0
        } else {
            self.file_index
                .trunk_position_for_record(self.current_index)
                .unwrap_or(0)
        };
        let mut header = format!(
            "Tinylog Viewer | file={} | records={} | trunks={}/{} | line={}",
            self.log_file,
            self.total_records,
            current_trunk,
            self.file_index.trunk_count(),
            self.current_index.saturating_add(1)
        );
        if let Some(search_state) = self.active_search.as_ref() {
            header.push_str(&format!(
                " | search=/{} | matches={}",
                search_state.keyword,
                search_state.ordered_matches.len()
            ));
        }
        if let Some(filter_state) = self.active_filter {
            header.push_str(&format!(" | filter=:{}", filter_state.level.command_name()));
        }
        header
    }

    /// Collects all rows that fit in the current viewport.
    fn collect_rows(
        &self,
        content_width: usize,
        visible_row_capacity: usize,
        visible_record_count: usize,
    ) -> Result<Vec<RenderedRow>, String> {
        let entries = self.read_entries(self.top_index, visible_record_count)?;
        self.build_rows_from_entries(self.top_index, &entries, content_width, visible_row_capacity)
    }

    /// Collects only entries matching the active level filter and expands outward by trunk.
    fn collect_filtered_rows(
        &self,
        content_width: usize,
        visible_row_capacity: usize,
    ) -> Result<Vec<RenderedRow>, String> {
        let Some(filter_state) = self.active_filter else {
            return Ok(Vec::new());
        };
        let current_trunk_index = self
            .file_index
            .trunk_index_for_record(self.current_index)
            .unwrap_or(0);
        let mut rows = Vec::new();
        for trunk_index in self.build_search_trunk_order(current_trunk_index) {
            let entries = format::read_trunk_entries(&self.file_index, trunk_index)?;
            let trunk_record_start = self
                .file_index
                .trunk_record_start(trunk_index)
                .ok_or_else(|| format!("invalid trunk index: {trunk_index}"))?;
            for entry_offset in self.filter_entry_order(trunk_index, trunk_record_start, entries.len()) {
                let entry = entries
                    .get(entry_offset)
                    .ok_or_else(|| "missing filtered trunk entry".to_string())?;
                if entry.level != filter_state.level {
                    continue;
                }
                let logical_index = trunk_record_start.saturating_add(entry_offset);
                let line_number = logical_index.saturating_add(1).to_string();
                let focus = if logical_index == self.current_index {
                    RowFocus::Focused
                } else {
                    RowFocus::Unfocused
                };
                let rendered_content = self.rendered_content(entry, content_width)?;
                self.push_entry_rows(
                    &mut rows,
                    visible_row_capacity,
                    &line_number,
                    rendered_content,
                    focus,
                );
                if rows.len() >= visible_row_capacity {
                    break;
                }
            }
            if rows.len() >= visible_row_capacity {
                break;
            }
        }
        self.pad_rows(&mut rows, visible_row_capacity);
        Ok(rows)
    }

    /// Renders the last page by traversing trunks backward from the persisted last-trunk pointer.
    fn render_bottom_rows(
        &self,
        content_width: usize,
        visible_row_capacity: usize,
        visible_record_count: usize,
    ) -> Result<(usize, Vec<RenderedRow>), String> {
        let mut entries =
            format::read_last_window_from_index(&self.file_index, visible_record_count)?.visible_entries;
        let mut top_index = self.total_records.saturating_sub(entries.len());
        while self.rendered_row_count_for_entries(&entries, content_width)? > visible_row_capacity
            && !entries.is_empty()
        {
            entries.remove(0);
            top_index = top_index.saturating_add(1);
        }
        let rows = self.build_rows_from_entries(top_index, &entries, content_width, visible_row_capacity)?;
        Ok((top_index, rows))
    }

    /// Builds all visible rows from one batch of already loaded logical entries.
    fn build_rows_from_entries(
        &self,
        start_index: usize,
        entries: &[format::ParsedLogEntry],
        content_width: usize,
        visible_row_capacity: usize,
    ) -> Result<Vec<RenderedRow>, String> {
        let mut rows = Vec::new();
        for (offset, entry) in entries.iter().enumerate() {
            if rows.len() >= visible_row_capacity {
                break;
            }
            let logical_index = start_index.saturating_add(offset);
            let line_number = logical_index.saturating_add(1).to_string();
            let focus = if logical_index == self.current_index {
                RowFocus::Focused
            } else {
                RowFocus::Unfocused
            };
            let rendered_content = self.rendered_content(entry, content_width)?;
            self.push_entry_rows(
                &mut rows,
                visible_row_capacity,
                &line_number,
                rendered_content,
                focus,
            );
        }
        self.pad_rows(&mut rows, visible_row_capacity);
        Ok(rows)
    }

    /// Appends the wrapped rows for one logical entry.
    fn push_entry_rows(
        &self,
        rows: &mut Vec<RenderedRow>,
        visible_row_capacity: usize,
        line_number: &str,
        rendered_content: Vec<WrappedContentLine>,
        focus: RowFocus,
    ) {
        for (rendered_index, rendered_line) in rendered_content.into_iter().enumerate() {
            rows.push(RenderedRow {
                line_number: if rendered_index == 0 {
                    Some(line_number.to_string())
                } else {
                    None
                },
                content: rendered_line.text,
                highlight_ranges: rendered_line.highlight_ranges,
                focus,
            });
            if rows.len() >= visible_row_capacity {
                break;
            }
        }
    }

    /// Pads the viewport with blank rows.
    fn pad_rows(&self, rows: &mut Vec<RenderedRow>, visible_row_capacity: usize) {
        let remaining = visible_row_capacity.saturating_sub(rows.len());
        for _ in 0..remaining {
            rows.push(RenderedRow {
                line_number: None,
                content: String::new(),
                highlight_ranges: Vec::new(),
                focus: RowFocus::Unfocused,
            });
        }
    }

    /// Formats one parsed entry into wrapped content rows.
    fn rendered_content(
        &self,
        entry: &format::ParsedLogEntry,
        content_width: usize,
    ) -> Result<Vec<WrappedContentLine>, String> {
        Ok(wrap_content_with_highlights(
            &self.entry_display_text(entry)?,
            self.active_search.as_ref().map(|search_state| search_state.keyword.as_str()),
            content_width,
        ))
    }

    /// Counts the physical rows required by the provided logical entries.
    fn rendered_row_count_for_entries(
        &self,
        entries: &[format::ParsedLogEntry],
        content_width: usize,
    ) -> Result<usize, String> {
        entries
            .iter()
            .map(|entry| self.rendered_content(entry, content_width).map(|rows| rows.len()))
            .try_fold(0usize, |total, row_count| {
                row_count.map(|count| total.saturating_add(count))
            })
    }

    /// Keeps the current row inside the viewport and only scrolls once it would cross an edge.
    fn resolve_top_index(
        &self,
        content_width: usize,
        visible_row_capacity: usize,
        visible_record_count: usize,
    ) -> Result<usize, String> {
        let mut top_index = if self.current_index < self.top_index {
            self.current_index
        } else {
            self.top_index
        };

        let min_top_for_current = self
            .current_index
            .saturating_add(1)
            .saturating_sub(visible_record_count.max(1));
        if top_index < min_top_for_current {
            top_index = min_top_for_current;
        }

        let last_row_index = visible_row_capacity.saturating_sub(1);
        let preceding_entry_count = self.current_index.saturating_sub(top_index);
        let preceding_entries = self.read_entries(top_index, preceding_entry_count)?;
        let rendered_row_counts = preceding_entries
            .iter()
            .map(|entry| self.rendered_content(entry, content_width).map(|rows| rows.len()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut rows_before_current = rendered_row_counts
            .iter()
            .copied()
            .fold(0usize, |total, count| total.saturating_add(count));
        let mut leading_entry_offset = 0usize;

        while rows_before_current > last_row_index && top_index < self.current_index {
            let top_rows = rendered_row_counts
                .get(leading_entry_offset)
                .copied()
                .ok_or_else(|| "missing rendered row count for visible entry".to_string())?;
            rows_before_current = rows_before_current.saturating_sub(top_rows);
            top_index = top_index.saturating_add(1);
            leading_entry_offset = leading_entry_offset.saturating_add(1);
        }

        Ok(top_index)
    }

    /// Reads one logical entry range and prefers tail traversal when the requested slice is near the file end.
    fn read_entries(
        &self,
        start_index: usize,
        entry_count: usize,
    ) -> Result<Vec<format::ParsedLogEntry>, String> {
        if entry_count == 0 || start_index >= self.total_records {
            return Ok(Vec::new());
        }

        let available_count = usize::min(entry_count, self.total_records.saturating_sub(start_index));
        Ok(format::read_visible_window_from_index(&self.file_index, start_index, available_count)?.visible_entries)
    }

    /// Searches for a keyword with trunk-based outward expansion and caches all result lines.
    pub fn search_with_progress<F>(
        &mut self,
        keyword: &str,
        mut report_progress: F,
    ) -> Result<SearchSummary, String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err("missing keyword after /".to_string());
        }

        let trunk_count = self.file_index.trunk_count();
        if trunk_count == 0 {
            self.active_search = None;
            return Err(format!("keyword not found: {keyword}"));
        }

        let current_trunk_index = self
            .file_index
            .trunk_index_for_record(self.current_index)
            .unwrap_or(0);
        let mut search_state = SearchState {
            keyword: keyword.to_string(),
            trunk_matches: vec![Vec::new(); trunk_count],
            ordered_matches: Vec::new(),
            current_match_index: 0,
            trunk_order: self.build_search_trunk_order(current_trunk_index),
            next_trunk_cursor: 0,
            total_trunks: trunk_count,
            origin_trunk_index: current_trunk_index,
        };

        if let Err(error) =
            self.advance_search_state_with_progress(&mut search_state, SearchAdvanceMode::Initial, &mut report_progress)
        {
            if error == "search canceled" {
                self.active_search = None;
            }
            return Err(error);
        }

        if search_state.ordered_matches.is_empty() {
            self.active_search = None;
            return Err(format!("keyword not found: {keyword}"));
        }

        self.sync_search_position(&mut search_state);
        let total_matches = search_state.ordered_matches.len();
        let keyword = search_state.keyword.clone();
        self.active_search = Some(search_state);

        Ok(SearchSummary {
            keyword,
            total_matches,
        })
    }

    /// Advances one active search until the requested pause condition is met.
    fn advance_search_with_progress<F>(
        &mut self,
        mode: SearchAdvanceMode,
        report_progress: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        let mut search_state = self.active_search.take().ok_or_else(|| "no active search".to_string())?;
        let result = self.advance_search_state_with_progress(&mut search_state, mode, report_progress);
        match result {
            Ok(()) => {
                self.sync_search_position(&mut search_state);
                self.active_search = Some(search_state);
                Ok(())
            }
            Err(error) if error == "search canceled" => {
                self.active_search = None;
                Err(error)
            }
            Err(error) => {
                self.active_search = Some(search_state);
                Err(error)
            }
        }
    }

    /// Scans additional trunks into one search state until the requested pause condition is met.
    fn advance_search_state_with_progress<F>(
        &mut self,
        search_state: &mut SearchState,
        mode: SearchAdvanceMode,
        report_progress: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&InteractiveViewerSession, SearchProgress) -> Result<SearchProgressAction, String>,
    {
        while search_state.next_trunk_cursor < search_state.trunk_order.len() {
            let trunk_index = search_state.trunk_order[search_state.next_trunk_cursor];
            let entries = format::read_trunk_entries(&self.file_index, trunk_index)?;
            let trunk_record_start = self
                .file_index
                .trunk_record_start(trunk_index)
                .ok_or_else(|| format!("invalid trunk index: {trunk_index}"))?;
            let mut matches = Vec::new();
            for (entry_offset, entry) in entries.iter().enumerate() {
                if Self::format_entry_display_text(entry)?.contains(&search_state.keyword) {
                    matches.push(trunk_record_start.saturating_add(entry_offset));
                }
            }
            search_state.trunk_matches[trunk_index] = matches.clone();
            search_state.ordered_matches.extend(matches.iter().copied());
            search_state.next_trunk_cursor = search_state.next_trunk_cursor.saturating_add(1);

            match report_progress(
                self,
                SearchProgress {
                    scanned_trunks: search_state.next_trunk_cursor,
                    total_trunks: search_state.total_trunks,
                    current_trunk_position: trunk_index.saturating_add(1),
                },
            )? {
                SearchProgressAction::Continue => {}
                SearchProgressAction::Cancel => return Err("search canceled".to_string()),
            }

            if self.should_pause_after_trunk(search_state, mode, trunk_index, &matches)? {
                break;
            }
        }
        Ok(())
    }

    /// Returns whether there are still unseen trunks for the active search.
    fn search_has_pending_trunks(&self) -> bool {
        self.active_search
            .as_ref()
            .map(|search_state| search_state.next_trunk_cursor < search_state.trunk_order.len())
            .unwrap_or(false)
    }

    /// Tries to move to the next cached result without scanning any new trunks.
    fn try_move_to_next_cached_search_result(&mut self) -> Result<bool, String> {
        let Some(search_state) = self.active_search.as_mut() else {
            return Err("no active search".to_string());
        };
        let Some(next_match_index) = search_state
            .ordered_matches
            .iter()
            .position(|match_index| *match_index > self.current_index)
        else {
            return Ok(false);
        };
        search_state.current_match_index = next_match_index;
        let target_index = search_state.ordered_matches[next_match_index];
        self.jump_to(u64::try_from(target_index).unwrap_or(u64::MAX))?;
        Ok(true)
    }

    /// Tries to move to the previous cached result without scanning any new trunks.
    fn try_move_to_previous_cached_search_result(&mut self) -> Result<bool, String> {
        let Some(search_state) = self.active_search.as_mut() else {
            return Err("no active search".to_string());
        };
        let Some(previous_match_index) = search_state
            .ordered_matches
            .iter()
            .rposition(|match_index| *match_index < self.current_index)
        else {
            return Ok(false);
        };
        search_state.current_match_index = previous_match_index;
        let target_index = search_state.ordered_matches[previous_match_index];
        self.jump_to(u64::try_from(target_index).unwrap_or(u64::MAX))?;
        Ok(true)
    }

    /// Keeps cached matches sorted and aligned with the current focused line.
    fn sync_search_position(&self, search_state: &mut SearchState) {
        search_state.ordered_matches.sort_unstable();
        if let Some(current_match_index) = search_state
            .ordered_matches
            .iter()
            .position(|match_index| *match_index == self.current_index)
        {
            search_state.current_match_index = current_match_index;
        }
    }

    /// Decides whether the current scan stage should stop after the latest trunk.
    fn should_pause_after_trunk(
        &mut self,
        search_state: &mut SearchState,
        mode: SearchAdvanceMode,
        trunk_index: usize,
        matches: &[usize],
    ) -> Result<bool, String> {
        Ok(match mode {
            SearchAdvanceMode::Initial => {
                let Some(target_index) =
                    self.select_initial_match(search_state.origin_trunk_index, trunk_index, matches)
                else {
                    return Ok(false);
                };
                self.jump_to(u64::try_from(target_index).unwrap_or(u64::MAX))?;
                true
            }
            SearchAdvanceMode::Next => search_state
                .ordered_matches
                .iter()
                .any(|match_index| *match_index > self.current_index),
            SearchAdvanceMode::Previous => search_state
                .ordered_matches
                .iter()
                .any(|match_index| *match_index < self.current_index),
        })
    }

    /// Renders one logical entry into the search/display text shown by the viewer.
    fn entry_display_text(&self, entry: &format::ParsedLogEntry) -> Result<String, String> {
        Self::format_entry_display_text(entry)
    }

    /// Renders one logical entry into the search/display text shown by the viewer.
    fn format_entry_display_text(entry: &format::ParsedLogEntry) -> Result<String, String> {
        Ok(format!(
            "{} {} {}",
            format::format_timestamp_millis(entry.timestamp_millis)?,
            entry.level.display_name(),
            entry.content
        ))
    }

    /// Builds the user-requested outward search order: N, N+1, N-1, N+2, N-2...
    fn build_search_trunk_order(&self, current_trunk_index: usize) -> Vec<usize> {
        let trunk_count = self.file_index.trunk_count();
        let mut order = Vec::with_capacity(trunk_count);
        order.push(current_trunk_index);
        for distance in 1..trunk_count {
            let forward = current_trunk_index.saturating_add(distance);
            if forward < trunk_count {
                order.push(forward);
            }
            if current_trunk_index >= distance {
                order.push(current_trunk_index - distance);
            }
        }
        order
    }

    /// Returns the entry scan order used by the active filter for one trunk.
    fn filter_entry_order(
        &self,
        trunk_index: usize,
        trunk_record_start: usize,
        entry_count: usize,
    ) -> Vec<usize> {
        let current_trunk_index = self
            .file_index
            .trunk_index_for_record(self.current_index)
            .unwrap_or(usize::MAX);
        if trunk_index == current_trunk_index {
            let current_local_offset = usize::min(self.current_index.saturating_sub(trunk_record_start), entry_count);
            return (current_local_offset..entry_count).chain(0..current_local_offset).collect();
        }
        if trunk_index > current_trunk_index {
            (0..entry_count).collect()
        } else {
            (0..entry_count).rev().collect()
        }
    }

    /// Resolves the first jump target inside one searched trunk.
    fn select_initial_match(
        &self,
        current_trunk_index: usize,
        trunk_index: usize,
        matches: &[usize],
    ) -> Option<usize> {
        if matches.is_empty() {
            return None;
        }
        if trunk_index == current_trunk_index {
            return matches
                .iter()
                .copied()
                .find(|match_index| *match_index > self.current_index)
                .or_else(|| matches.iter().copied().rev().find(|match_index| *match_index < self.current_index))
                .or_else(|| matches.first().copied());
        }
        if trunk_index > current_trunk_index {
            matches.first().copied()
        } else {
            matches.last().copied()
        }
    }
}

/// Wraps one logical log line into the right-hand content area while preserving explicit newlines.
fn wrap_content_with_highlights(
    content: &str,
    keyword: Option<&str>,
    content_width: usize,
) -> Vec<WrappedContentLine> {
    let global_highlight_ranges = keyword
        .filter(|keyword| !keyword.is_empty())
        .map(|keyword| find_highlight_ranges(content, keyword))
        .unwrap_or_default();
    let mut rendered_lines = Vec::new();
    let mut segment_start_char = 0usize;
    for segment in content.split('\n') {
        if segment.is_empty() {
            rendered_lines.push(WrappedContentLine {
                text: String::new(),
                highlight_ranges: Vec::new(),
            });
            segment_start_char = segment_start_char.saturating_add(1);
            continue;
        }
        let characters: Vec<char> = segment.chars().collect();
        let mut start = 0usize;
        while start < characters.len() {
            let end = usize::min(start.saturating_add(content_width), characters.len());
            rendered_lines.push(WrappedContentLine {
                text: characters[start..end].iter().collect(),
                highlight_ranges: intersect_highlight_ranges(
                    &global_highlight_ranges,
                    segment_start_char.saturating_add(start),
                    segment_start_char.saturating_add(end),
                ),
            });
            start = end;
        }
        segment_start_char = segment_start_char.saturating_add(characters.len()).saturating_add(1);
    }
    if rendered_lines.is_empty() {
        rendered_lines.push(WrappedContentLine {
            text: String::new(),
            highlight_ranges: Vec::new(),
        });
    }
    rendered_lines
}

/// Stores one wrapped content row plus highlight ranges local to that row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WrappedContentLine {
    text: String,
    highlight_ranges: Vec<(usize, usize)>,
}

/// Finds all search match ranges as character offsets.
fn find_highlight_ranges(content: &str, keyword: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut search_start = 0usize;
    while let Some(relative_index) = content[search_start..].find(keyword) {
        let byte_start = search_start.saturating_add(relative_index);
        let byte_end = byte_start.saturating_add(keyword.len());
        let char_start = content[..byte_start].chars().count();
        let char_end = content[..byte_end].chars().count();
        ranges.push((char_start, char_end));
        search_start = byte_start.saturating_add(keyword.len());
    }
    ranges
}

/// Intersects global highlight ranges with one wrapped line.
fn intersect_highlight_ranges(
    ranges: &[(usize, usize)],
    line_start: usize,
    line_end: usize,
) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .filter_map(|(range_start, range_end)| {
            if *range_end <= line_start || *range_start >= line_end {
                return None;
            }
            Some((
                range_start.saturating_sub(line_start),
                usize::min(*range_end, line_end).saturating_sub(line_start),
            ))
        })
        .collect()
}

impl ViewerSession for InteractiveViewerSession {
    fn log_file(&self) -> Option<&str> {
        Some(&self.log_file)
    }

    fn browse(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn search(&mut self, _keyword: &str) -> Result<(), String> {
        self.search_with_progress(_keyword, |_session, _progress| Ok(SearchProgressAction::Continue))
            .map(|_| ())
    }

    fn jump_to(&mut self, offset: u64) -> Result<(), String> {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        self.current_index = usize::min(offset, self.total_records.saturating_sub(1));
        self.top_index = self.current_index;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tinylog_rust_common::format::LogLevel;
    use crate::viewer::session::{
        InteractiveViewerSession, RowFocus, SearchProgress, SearchProgressAction, ViewerSession,
    };

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
        trunk.push(2);
        trunk.extend_from_slice(&5_u32.to_be_bytes());
        trunk.extend_from_slice(b"alpha");
        trunk.extend_from_slice(&25_u32.to_be_bytes());
        trunk.push(2);
        trunk.extend_from_slice(&4_u32.to_be_bytes());
        trunk.extend_from_slice(b"beta");
        trunk.extend_from_slice(&50_u32.to_be_bytes());
        trunk.push(2);
        trunk.extend_from_slice(&5_u32.to_be_bytes());
        trunk.extend_from_slice(b"gamma");
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        bytes
    }

    /// Builds one valid multi-trunk prototype file from per-trunk message lists.
    fn sample_bytes_with_trunks(trunks: &[&[&str]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&512_u16.to_be_bytes());
        bytes.extend_from_slice(&1_777_672_860_253_u64.to_be_bytes());
        let total_records = trunks.iter().map(|trunk| trunk.len()).sum::<usize>();
        bytes.extend_from_slice(&(total_records as u64).to_be_bytes());
        bytes.extend_from_slice(&(trunks.len() as u32).to_be_bytes()[1..]);

        let mut offset_millis = 0u32;
        for trunk_entries in trunks {
            bytes.extend_from_slice(&(trunk_entries.len() as u16).to_be_bytes());
            let mut trunk = Vec::new();
            for entry in *trunk_entries {
                trunk.extend_from_slice(&offset_millis.to_be_bytes());
                trunk.push(2);
                trunk.extend_from_slice(&(entry.len() as u32).to_be_bytes());
                trunk.extend_from_slice(entry.as_bytes());
                offset_millis = offset_millis.saturating_add(25);
            }
            bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&trunk);
        }

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

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 2)
            .expect("open session");
        let first_page = session.render_frame(4, 80).expect("render first page");
        session.move_down();
        let second_page = session.render_frame(4, 80).expect("render second page");

        assert!(first_page.header.contains("Tinylog Viewer"));
        assert!(first_page.header.contains("trunks=1/1"));
        assert_eq!(first_page.rows[0].line_number.as_deref(), Some("1"));
        assert!(first_page.rows[0]
            .content
            .contains("2026-05-01 22:01:00,253 [INFO] alpha"));
        assert_eq!(first_page.rows[0].focus, RowFocus::Focused);
        assert_eq!(first_page.rows[1].line_number.as_deref(), Some("2"));
        assert!(first_page.rows[1]
            .content
            .contains("2026-05-01 22:01:00,278 [INFO] beta"));
        assert!(!first_page
            .rows
            .iter()
            .any(|row| row.content.contains("gamma")));
        assert_eq!(session.top_index(), 0);
        assert_eq!(second_page.rows[0].line_number.as_deref(), Some("1"));
        assert_eq!(second_page.rows[1].line_number.as_deref(), Some("2"));
        assert_eq!(second_page.rows[1].focus, RowFocus::Focused);

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

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 2)
            .expect("open session");
        session.move_to_bottom();
        let _ = session.render_frame(4, 80).expect("render bottom page");

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

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 8)
            .expect("open session");
        session.quarter_page_down(10);
        let _ = session.render_frame(10, 80).expect("render quarter page");

        assert_eq!(session.current_index(), 2);
        assert_eq!(session.top_index(), 0);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn move_down_scrolls_only_after_current_row_hits_bottom_edge() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-focus-{}.tog",
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
        bytes.extend_from_slice(&8_u64.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&8_u16.to_be_bytes());
        let mut trunk = Vec::new();
        for index in 0..8_u32 {
            let content = format!("line-{index}");
            trunk.extend_from_slice(&(index * 25).to_be_bytes());
            trunk.push(2);
            trunk.extend_from_slice(&(content.len() as u32).to_be_bytes());
            trunk.extend_from_slice(content.as_bytes());
        }
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        fs::write(&path, bytes).expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 8)
            .expect("open session");
        for _ in 0..3 {
            session.move_down();
        }
        let bottom_frame = session
            .render_frame(5, 80)
            .expect("render bottom edge page");
        session.move_down();
        let scrolled_frame = session.render_frame(5, 80).expect("render scrolled page");

        assert_eq!(session.current_index(), 4);
        assert_eq!(session.top_index(), 1);
        assert_eq!(bottom_frame.rows[3].line_number.as_deref(), Some("4"));
        assert_eq!(bottom_frame.rows[3].focus, RowFocus::Focused);
        assert_eq!(scrolled_frame.rows[3].line_number.as_deref(), Some("5"));
        assert_eq!(scrolled_frame.rows[3].focus, RowFocus::Focused);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn move_up_from_bottom_keeps_last_page_window() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-tail-{}.tog",
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
        bytes.extend_from_slice(&8_u64.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&8_u16.to_be_bytes());
        let mut trunk = Vec::new();
        for index in 0..8_u32 {
            let content = format!("tail-line-{index}");
            trunk.extend_from_slice(&(index * 25).to_be_bytes());
            trunk.push(2);
            trunk.extend_from_slice(&(content.len() as u32).to_be_bytes());
            trunk.extend_from_slice(content.as_bytes());
        }
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        fs::write(&path, bytes).expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        session.move_to_bottom();
        let bottom_frame = session.render_frame(5, 80).expect("render bottom page");
        session.move_up();
        let previous_frame = session.render_frame(5, 80).expect("render previous row");

        assert_eq!(session.current_index(), 6);
        assert_eq!(session.top_index(), 4);
        assert_eq!(bottom_frame.rows[3].line_number.as_deref(), Some("8"));
        assert_eq!(previous_frame.rows[2].line_number.as_deref(), Some("7"));
        assert_eq!(previous_frame.rows[2].focus, RowFocus::Focused);

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
        trunk.push(2);
        trunk.extend_from_slice(&(message.len() as u32).to_be_bytes());
        trunk.extend_from_slice(message);
        bytes.extend_from_slice(&(trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk);
        fs::write(&path, bytes).expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        let rendered = session.render_frame(8, 24).expect("render wrapped page");

        assert_eq!(rendered.rows[0].line_number.as_deref(), Some("1"));
        assert!(rendered.rows[0].content.starts_with("2026-05-01"));
        assert_eq!(rendered.rows[0].focus, RowFocus::Focused);
        assert_eq!(rendered.rows[1].line_number, None);
        assert!(!rendered.rows[1].content.is_empty());
        assert_eq!(rendered.rows[1].focus, RowFocus::Focused);
        assert!(rendered
            .rows
            .iter()
            .any(|row| row.line_number.is_none() && row.content.contains("gamma delta")));

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn search_prefers_next_match_in_current_trunk_then_navigates_cached_results() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-search-current-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            sample_bytes_with_trunks(&[&["before-hit", "focus-line", "after-hit"]]),
        )
        .expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        session.move_down();
        let mut progress = Vec::new();
        let summary = session
            .search_with_progress("hit", |_session, current_progress| {
                progress.push((current_progress.scanned_trunks, current_progress.total_trunks));
                Ok(SearchProgressAction::Continue)
            })
            .expect("search current trunk");
        let rendered = session.render_frame(6, 80).expect("render search result");

        assert_eq!(summary.total_matches, 2);
        assert_eq!(progress, vec![(1, 1)]);
        assert_eq!(session.current_index(), 2);
        assert_eq!(
            session.search_trunk_matches().expect("search cache"),
            &[vec![0, 2]]
        );
        assert!(rendered.header.contains("search=/hit"));
        assert!(rendered.rows.iter().any(|row| !row.highlight_ranges.is_empty()));

        session
            .move_to_previous_search_result_with_progress(|_session, _progress| {
                Ok(SearchProgressAction::Continue)
            })
            .expect("move to previous");
        assert_eq!(session.current_index(), 0);
        session
            .move_to_next_search_result_with_progress(|_session, _progress| {
                Ok(SearchProgressAction::Continue)
            })
            .expect("move to next");
        assert_eq!(session.current_index(), 2);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn search_prefers_forward_trunk_before_backward_trunk() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-search-trunks-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            sample_bytes_with_trunks(&[
                &["backward-hit"],
                &["center-a", "center-b"],
                &["forward-hit"],
            ]),
        )
        .expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        session.move_down();
        let summary = session
            .search_with_progress("hit", |_session, _progress| Ok(SearchProgressAction::Continue))
            .expect("search outward trunks");

        assert_eq!(summary.total_matches, 1);
        assert_eq!(session.current_index(), 3);
        assert_eq!(
            session.search_trunk_matches().expect("search cache"),
            &[vec![], vec![], vec![3]]
        );
        session
            .move_to_previous_search_result_with_progress(|_session, _progress| {
                Ok(SearchProgressAction::Continue)
            })
            .expect("continue search backward");
        assert_eq!(session.current_index(), 0);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn search_cancellation_clears_active_search_state() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-search-cancel-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            sample_bytes_with_trunks(&[&["alpha"], &["beta-hit"], &["gamma-hit"]]),
        )
        .expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        let error = session
            .search_with_progress("hit", |_session, _progress| Ok(SearchProgressAction::Cancel))
            .expect_err("cancel search");

        assert_eq!(error, "search canceled");
        assert!(session.search_trunk_matches().is_none());

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn search_progress_reports_integer_percentage() {
        assert_eq!(
            SearchProgress {
                scanned_trunks: 121,
                total_trunks: 200,
                current_trunk_position: 121,
            }
            .percentage(),
            60
        );
    }

    #[test]
    fn search_starts_from_large_current_trunk_before_head_trunk() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-search-large-order-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut trunks = vec![vec!["other".to_string(); 1]; 130];
        trunks[119] = vec!["backward-hit".to_string()];
        trunks[120] = vec!["focus".to_string()];
        trunks[121] = vec!["forward-hit".to_string()];
        trunks[0] = vec!["head-hit".to_string()];
        let trunk_refs = trunks
            .iter()
            .map(|trunk| trunk.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let trunk_slices = trunk_refs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        fs::write(&path, sample_bytes_with_trunks(&trunk_slices)).expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        session.jump_to(120).expect("jump to trunk 121");
        let summary = session
            .search_with_progress("hit", |_session, _progress| Ok(SearchProgressAction::Continue))
            .expect("search large trunk order");

        assert_eq!(summary.total_matches, 1);
        assert_eq!(session.current_index(), 121);

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn level_filter_only_shows_matching_rows_and_keeps_original_line_numbers() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-session-level-filter-{}.tog",
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
        bytes.extend_from_slice(&4_u64.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 2]);

        let mut trunk_one = Vec::new();
        trunk_one.extend_from_slice(&0_u32.to_be_bytes());
        trunk_one.push(2);
        trunk_one.extend_from_slice(&4_u32.to_be_bytes());
        trunk_one.extend_from_slice(b"info");
        trunk_one.extend_from_slice(&25_u32.to_be_bytes());
        trunk_one.push(1);
        trunk_one.extend_from_slice(&5_u32.to_be_bytes());
        trunk_one.extend_from_slice(b"debug");
        bytes.extend_from_slice(&2_u16.to_be_bytes());
        bytes.extend_from_slice(&(trunk_one.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk_one);

        let mut trunk_two = Vec::new();
        trunk_two.extend_from_slice(&50_u32.to_be_bytes());
        trunk_two.push(3);
        trunk_two.extend_from_slice(&4_u32.to_be_bytes());
        trunk_two.extend_from_slice(b"warn");
        trunk_two.extend_from_slice(&75_u32.to_be_bytes());
        trunk_two.push(1);
        trunk_two.extend_from_slice(&6_u32.to_be_bytes());
        trunk_two.extend_from_slice(b"debug2");
        bytes.extend_from_slice(&2_u16.to_be_bytes());
        bytes.extend_from_slice(&(trunk_two.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&trunk_two);

        fs::write(&path, bytes).expect("write prototype file");

        let mut session = InteractiveViewerSession::open(path.to_string_lossy().into_owned(), 4)
            .expect("open session");
        session.apply_level_filter(LogLevel::Debug);
        let frame = session.render_frame(6, 80).expect("render filtered page");

        assert!(frame.header.contains("filter=:debug"));
        assert_eq!(frame.rows[0].line_number.as_deref(), Some("2"));
        assert!(frame.rows[0].content.contains("[DEBUG] debug"));
        assert_eq!(frame.rows[1].line_number.as_deref(), Some("4"));
        assert!(frame.rows[1].content.contains("[DEBUG] debug2"));
        assert!(frame.rows[2].line_number.is_none());

        fs::remove_file(path).expect("cleanup file");
    }
}
