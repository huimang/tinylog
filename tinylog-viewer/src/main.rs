mod app;
mod config;
mod format;
mod session;

use app::ViewerApplication;
use config::ViewerConfig;

/// Starts the tinylog viewer scaffold and accepts an optional log file path.
fn main() {
    let mut config = ViewerConfig::default();

    if let Some(path) = std::env::args().nth(1) {
        config.log_file = Some(path);
    }

    match ViewerApplication::new(config).run() {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
