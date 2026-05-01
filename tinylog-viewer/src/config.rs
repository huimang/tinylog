/// Holds user-visible viewer behavior without exposing internal I/O strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerConfig {
    pub log_file: Option<String>,
    pub page_size: usize,
    pub prefetch_pages: usize,
}

impl Default for ViewerConfig {
    /// Provides the baseline browsing configuration for the scaffold.
    fn default() -> Self {
        Self {
            log_file: None,
            page_size: 200,
            prefetch_pages: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ViewerConfig;

    #[test]
    fn default_config_matches_scaffold_expectation() {
        let config = ViewerConfig::default();

        assert_eq!(config.page_size, 200);
        assert_eq!(config.prefetch_pages, 2);
        assert_eq!(config.log_file, None);
    }
}
