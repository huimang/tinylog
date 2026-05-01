use crate::format;
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

    /// Opens the configured file and renders a prototype browsing result.
    pub fn run(&self) -> Result<String, String> {
        match self.config.log_file.as_deref() {
            Some(path) => {
                let entries = format::read_file(path)?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "tinylog viewer opened {path} with {} records.",
                    entries.len()
                ));
                for entry in entries.iter().take(self.config.page_size) {
                    lines.push(format!(
                        "{} {}",
                        format::format_timestamp_millis(entry.timestamp_millis)?,
                        entry.content
                    ));
                }
                if entries.len() > self.config.page_size {
                    lines.push(format!(
                        "... {} more records omitted",
                        entries.len() - self.config.page_size
                    ));
                }
                Ok(lines.join("\n"))
            }
            None => Ok(self.banner()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::ViewerConfig;
    use crate::format;

    use super::ViewerApplication;

    /**
     * Builds one valid two-record prototype file for viewer-side tests.
     */
    fn sample_bytes() -> Vec<u8> {
        vec![
            0, 0, 1, 139, 207, 229, 104, 0,
            0, 0, 0, 0, 0, 0, 0, 2,
            0, 0, 0, 0, 0, 0, 5, b'a', b'l', b'p', b'h', b'a',
            0, 0, 0, 25, 0, 0, 4, b'b', b'e', b't', b'a',
        ]
    }

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

    #[test]
    fn run_renders_records_from_prototype_file() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-viewer-test-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            sample_bytes(),
        )
        .expect("write prototype file");

        let mut config = ViewerConfig::default();
        config.log_file = Some(path.to_string_lossy().into_owned());

        let output = ViewerApplication::new(config).run().expect("viewer output");

        assert!(output.contains("tinylog viewer opened"));
        assert!(output.contains(&format::format_timestamp_millis(1_700_000_000_000).expect("format time")));
        assert!(!output.contains("+25ms"));
        assert!(output.contains("alpha"));
        assert!(output.contains("beta"));

        fs::remove_file(path).expect("cleanup file");
    }
}
