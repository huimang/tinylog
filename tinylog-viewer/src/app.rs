use crate::config::ViewerConfig;

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
}

#[cfg(test)]
mod tests {
    use crate::config::ViewerConfig;

    use super::ViewerApplication;

    #[test]
    fn banner_contains_target_file_when_provided() {
        let mut config = ViewerConfig::default();
        config.log_file = Some("demo.tlog".to_string());

        let app = ViewerApplication::new(config);

        assert_eq!(
            app.banner(),
            "tinylog viewer scaffold initialized for demo.tlog."
        );
    }
}
