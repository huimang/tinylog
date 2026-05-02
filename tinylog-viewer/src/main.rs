use tinylog_viewer::viewer::{app::ViewerApplication, config::ViewerConfig};

/// Starts the TinyLog viewer scaffold and accepts an optional log file path.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut config = ViewerConfig::default();
    if let Some(path) = arguments.first() {
        config.log_file = Some(path.clone());
    }

    match ViewerApplication::new(config).run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
