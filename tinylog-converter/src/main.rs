/// Starts the TinyLog converter CLI.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = tinylog_converter::run_convert_cli(&arguments) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
