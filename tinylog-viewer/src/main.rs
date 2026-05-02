mod app;
mod config;
mod converter;
mod format;
mod session;

use app::ViewerApplication;
use config::ViewerConfig;

/// Starts the TinyLog viewer scaffold and accepts an optional log file path.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("convert") {
        if let Err(error) = converter::run_convert_cli(&arguments[1..]) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

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
