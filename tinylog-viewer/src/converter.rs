use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::{NaiveDateTime, TimeZone, Utc};

use crate::format::{CompressionAlgorithm, LogLevel};

const FILE_EXTENSION: &str = ".tog";
const DEFAULT_TRUNK_SIZE_KB: u16 = 512;
const HEADER_BYTES: u64 = 26;
const BASE_TIMESTAMP_OFFSET: u64 = 7;
const TOTAL_LOG_LINE_COUNT_OFFSET: u64 = 15;
const MAX_OFFSET_MILLIS: u64 = 0xFFFF_FFFF;
const MAX_TRUNK_LOG_LINE_COUNT: u16 = 0xFFFF;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S,%3f";
const TIMESTAMP_TEXT_LENGTH: usize = 23;
const TIMESTAMP_SEPARATOR: char = ' ';
const LINE_HEADER_BYTES: usize = 9;
const BYTES_PER_KB: usize = 1024;
const PROGRESS_UPDATE_STEP: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogRecord {
    timestamp_millis: u64,
    level: LogLevel,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConversionSummary {
    source_size_bytes: u64,
    output_size_bytes: u64,
}

/// Runs the Rust plaintext-to-TinyLog converter CLI.
pub fn run_convert_cli(arguments: &[String]) -> Result<(), String> {
    if arguments.len() < 2 || arguments.len() > 4 {
        return Err(
            "usage: tinylog-viewer convert <input.log> <output.tog> [algorithmId] [trunkSizeKb]"
                .to_string(),
        );
    }

    let input_path = Path::new(&arguments[0]);
    let output_path = Path::new(&arguments[1]);
    let compression_algorithm = parse_compression_algorithm(arguments)?;
    let trunk_size_kb = parse_trunk_size_kb(arguments)?;
    let mut progress_reporter = ProgressReporter::new(io::stderr());
    let started_at = Instant::now();

    let summary = convert_plaintext_log(
        input_path,
        output_path,
        compression_algorithm,
        trunk_size_kb,
        &mut progress_reporter,
    )?;
    let elapsed = started_at.elapsed();

    println!(
        "converted {} to {} using {}",
        input_path.display(),
        output_path.display(),
        compression_algorithm.display_name()
    );
    print_conversion_summary(&summary, elapsed);
    Ok(())
}

fn parse_compression_algorithm(arguments: &[String]) -> Result<CompressionAlgorithm, String> {
    if arguments.len() < 3 {
        return Ok(CompressionAlgorithm::Gzip);
    }

    let algorithm_id = arguments[2]
        .parse::<u16>()
        .map_err(|error| format!("invalid algorithmId '{}': {error}", arguments[2]))?;
    CompressionAlgorithm::from_id(algorithm_id)
}

fn parse_trunk_size_kb(arguments: &[String]) -> Result<u16, String> {
    if arguments.len() < 4 {
        return Ok(DEFAULT_TRUNK_SIZE_KB);
    }

    let trunk_size_kb = arguments[3]
        .parse::<u16>()
        .map_err(|error| format!("invalid trunkSizeKb '{}': {error}", arguments[3]))?;
    validate_trunk_size_kb(trunk_size_kb)?;
    Ok(trunk_size_kb)
}

fn convert_plaintext_log(
    plain_text_log_path: &Path,
    tinylog_path: &Path,
    compression_algorithm: CompressionAlgorithm,
    trunk_size_kb: u16,
    progress_reporter: &mut ProgressReporter<impl Write>,
) -> Result<ConversionSummary, String> {
    validate_tinylog_path(tinylog_path)?;
    let source_size_bytes = file_size_bytes(plain_text_log_path)?;
    progress_reporter.write_counting_message(plain_text_log_path)?;

    let total_lines = count_total_lines(plain_text_log_path)?;
    progress_reporter.start(total_lines)?;

    let reader = open_buffered_reader(plain_text_log_path)?;
    let mut writer = TinylogWriter::new(tinylog_path, compression_algorithm, trunk_size_kb)?;
    let mut line = String::new();
    let mut reader = reader;
    let mut line_number = 0usize;
    let mut processed_lines = 0u64;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read {}: {error}", plain_text_log_path.display()))?;
        if bytes_read == 0 {
            break;
        }
        trim_line_ending(&mut line);
        line_number += 1;
        processed_lines += 1;

        if !line.trim().is_empty() {
            let record = parse_line(plain_text_log_path, line_number, &line)?;
            writer.append(record)?;
        }
        progress_reporter.maybe_render(processed_lines, total_lines)?;
    }

    writer.close()?;
    progress_reporter.finish(total_lines)?;
    let output_size_bytes = file_size_bytes(tinylog_path)?;
    Ok(ConversionSummary {
        source_size_bytes,
        output_size_bytes,
    })
}

fn validate_tinylog_path(tinylog_path: &Path) -> Result<(), String> {
    let file_name = tinylog_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid output path: {}", tinylog_path.display()))?;
    if !file_name.ends_with(FILE_EXTENSION) {
        return Err(format!("TinyLog files must use the {FILE_EXTENSION} extension"));
    }
    Ok(())
}

fn count_total_lines(path: &Path) -> Result<u64, String> {
    let mut reader = open_buffered_reader(path)?;
    let mut line = String::new();
    let mut total_lines = 0u64;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        total_lines += 1;
    }

    Ok(total_lines)
}

fn open_buffered_reader(path: &Path) -> Result<BufReader<File>, String> {
    let file = File::open(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(BufReader::new(file))
}

fn file_size_bytes(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| format!("failed to read {} metadata: {error}", path.display()))
}

fn trim_line_ending(line: &mut String) {
    if line.ends_with('\n') {
        line.pop();
    }
    if line.ends_with('\r') {
        line.pop();
    }
}

fn parse_line(path: &Path, line_number: usize, line: &str) -> Result<LogRecord, String> {
    validate_line_shape(path, line_number, line)?;
    let timestamp_millis = parse_timestamp_millis(path, line_number, line)?;
    let (level, message) = parse_level_and_message(line);
    Ok(LogRecord {
        timestamp_millis,
        level,
        message,
    })
}

fn validate_line_shape(path: &Path, line_number: usize, line: &str) -> Result<(), String> {
    if line.len() <= TIMESTAMP_TEXT_LENGTH + 1 || line.as_bytes()[TIMESTAMP_TEXT_LENGTH] != b' ' {
        return Err(format!(
            "invalid log line at {}:{line_number}, expected '<yyyy-MM-dd HH:mm:ss,SSS> <message>'",
            path.display()
        ));
    }
    Ok(())
}

fn parse_timestamp_millis(path: &Path, line_number: usize, line: &str) -> Result<u64, String> {
    let date_time = NaiveDateTime::parse_from_str(&line[..TIMESTAMP_TEXT_LENGTH], TIMESTAMP_FORMAT)
        .map_err(|_| format!("invalid timestamp at {}:{line_number}", path.display()))?;
    let timestamp_millis = Utc.from_utc_datetime(&date_time).timestamp_millis();
    u64::try_from(timestamp_millis)
        .map_err(|_| format!("timestamp before unix epoch at {}:{line_number}", path.display()))
}

fn parse_level_and_message(line: &str) -> (LogLevel, String) {
    let content = &line[TIMESTAMP_TEXT_LENGTH + 1..];
    if !content.starts_with('[') {
        return (LogLevel::Info, content.to_string());
    }

    let Some(closing_bracket_index) = content.find(']') else {
        return (LogLevel::Info, content.to_string());
    };

    let Some(level) = LogLevel::parse_token(&content[1..closing_bracket_index]) else {
        return (LogLevel::Info, content.to_string());
    };

    let mut message = content[closing_bracket_index + 1..].to_string();
    if message.starts_with(TIMESTAMP_SEPARATOR) {
        message.remove(0);
    }
    (level, message)
}

fn validate_trunk_size_kb(trunk_size_kb: u16) -> Result<(), String> {
    if trunk_size_kb == 0 {
        return Err("trunk size must be greater than zero".to_string());
    }
    Ok(())
}

fn current_format_version() -> Result<[u8; 3], String> {
    let version_text = env!("CARGO_PKG_VERSION");
    let version_without_suffix = version_text.split('-').next().unwrap_or(version_text);
    let mut version = [0_u8; 3];

    for (index, segment) in version_without_suffix.split('.').take(version.len()).enumerate() {
        let value = segment
            .parse::<u16>()
            .map_err(|error| format!("invalid tinylog version segment '{segment}': {error}"))?;
        if value > u16::from(u8::MAX) {
            return Err(format!(
                "tinylog version segment must fit in one byte: {segment}"
            ));
        }
        version[index] = value as u8;
    }

    Ok(version)
}

fn write_u24(target: &mut impl Write, value: u32) -> Result<(), String> {
    if value > 0xFF_FFFF {
        return Err("value must fit in 3 bytes".to_string());
    }
    target
        .write_all(&[
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
        ])
        .map_err(|error| format!("failed to write header field: {error}"))
}

fn line_byte_size(record: &LogRecord) -> usize {
    LINE_HEADER_BYTES + record.message.as_bytes().len()
}

fn write_raw_log_line(target: &mut Vec<u8>, record: &LogRecord, base_timestamp_millis: u64) -> Result<(), String> {
    let offset_millis = record
        .timestamp_millis
        .checked_sub(base_timestamp_millis)
        .ok_or_else(|| "records must be appended in timestamp order".to_string())?;
    if offset_millis > MAX_OFFSET_MILLIS {
        return Err("record offset must fit in 4 bytes".to_string());
    }

    let content_bytes = record.message.as_bytes();
    let content_length = u32::try_from(content_bytes.len())
        .map_err(|_| "log line content length must fit in 4 bytes".to_string())?;
    target.extend_from_slice(&(offset_millis as u32).to_be_bytes());
    target.push(record.level.to_persisted_id());
    target.extend_from_slice(&content_length.to_be_bytes());
    target.extend_from_slice(content_bytes);
    Ok(())
}

struct TinylogWriter {
    main_file: File,
    compression_algorithm: CompressionAlgorithm,
    trunk_size_bytes: usize,
    base_timestamp_millis: Option<u64>,
    last_timestamp_millis: Option<u64>,
    total_log_line_count: u64,
    trunk_count: u32,
    current_trunk_line_count: u16,
    current_trunk_bytes: usize,
    raw_trunk_buffer: Vec<u8>,
}

impl TinylogWriter {
    fn new(path: &Path, compression_algorithm: CompressionAlgorithm, trunk_size_kb: u16) -> Result<Self, String> {
        validate_trunk_size_kb(trunk_size_kb)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        let mut main_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        write_header(&mut main_file, compression_algorithm, trunk_size_kb, 0, 0, 0)?;

        Ok(Self {
            main_file,
            compression_algorithm,
            trunk_size_bytes: usize::from(trunk_size_kb) * BYTES_PER_KB,
            base_timestamp_millis: None,
            last_timestamp_millis: None,
            total_log_line_count: 0,
            trunk_count: 0,
            current_trunk_line_count: 0,
            current_trunk_bytes: 0,
            raw_trunk_buffer: Vec::with_capacity(usize::from(trunk_size_kb) * BYTES_PER_KB),
        })
    }

    fn append(&mut self, record: LogRecord) -> Result<(), String> {
        self.ensure_timestamp_order(&record)?;
        self.initialize_base_timestamp(&record)?;

        if self.current_trunk_line_count == MAX_TRUNK_LOG_LINE_COUNT {
            self.flush_current_trunk()?;
        }

        let line_bytes = line_byte_size(&record);
        if self.current_trunk_line_count > 0 && self.current_trunk_bytes + line_bytes > self.trunk_size_bytes {
            self.flush_current_trunk()?;
        }

        let base_timestamp_millis = self.base_timestamp_millis.unwrap_or(record.timestamp_millis);
        write_raw_log_line(&mut self.raw_trunk_buffer, &record, base_timestamp_millis)?;
        self.current_trunk_bytes += line_bytes;
        self.current_trunk_line_count += 1;
        self.last_timestamp_millis = Some(record.timestamp_millis);

        if self.current_trunk_bytes >= self.trunk_size_bytes {
            self.flush_current_trunk()?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), String> {
        self.flush_current_trunk()
    }

    fn ensure_timestamp_order(&self, record: &LogRecord) -> Result<(), String> {
        if let Some(last_timestamp_millis) = self.last_timestamp_millis {
            if record.timestamp_millis < last_timestamp_millis {
                return Err("records must be appended in timestamp order".to_string());
            }
        }
        Ok(())
    }

    fn initialize_base_timestamp(&mut self, record: &LogRecord) -> Result<(), String> {
        if self.base_timestamp_millis.is_some() {
            return Ok(());
        }

        self.base_timestamp_millis = Some(record.timestamp_millis);
        self.main_file
            .seek(SeekFrom::Start(BASE_TIMESTAMP_OFFSET))
            .map_err(|error| format!("failed to update base timestamp: {error}"))?;
        self.main_file
            .write_all(&record.timestamp_millis.to_be_bytes())
            .map_err(|error| format!("failed to update base timestamp: {error}"))?;
        self.main_file
            .seek(SeekFrom::End(0))
            .map_err(|error| format!("failed to restore file position: {error}"))?;
        Ok(())
    }

    fn flush_current_trunk(&mut self) -> Result<(), String> {
        if self.current_trunk_line_count == 0 {
            return Ok(());
        }

        let compressed_trunk_bytes = self
            .compression_algorithm
            .compress(&self.raw_trunk_buffer)?;
        self.main_file
            .seek(SeekFrom::End(0))
            .map_err(|error| format!("failed to append trunk: {error}"))?;
        self.main_file
            .write_all(&self.current_trunk_line_count.to_be_bytes())
            .map_err(|error| format!("failed to write trunk line count: {error}"))?;
        self.main_file
            .write_all(
                &(u32::try_from(compressed_trunk_bytes.len())
                    .map_err(|_| "compressed trunk length must fit in 4 bytes".to_string())?)
                .to_be_bytes(),
            )
            .map_err(|error| format!("failed to write trunk length: {error}"))?;
        self.main_file
            .write_all(&compressed_trunk_bytes)
            .map_err(|error| format!("failed to write trunk payload: {error}"))?;

        self.total_log_line_count += u64::from(self.current_trunk_line_count);
        self.trunk_count += 1;
        self.update_header_counters()?;
        self.raw_trunk_buffer.clear();
        self.current_trunk_line_count = 0;
        self.current_trunk_bytes = 0;
        Ok(())
    }

    fn update_header_counters(&mut self) -> Result<(), String> {
        self.main_file
            .seek(SeekFrom::Start(TOTAL_LOG_LINE_COUNT_OFFSET))
            .map_err(|error| format!("failed to update header counters: {error}"))?;
        self.main_file
            .write_all(&self.total_log_line_count.to_be_bytes())
            .map_err(|error| format!("failed to update total log line count: {error}"))?;
        write_u24(&mut self.main_file, self.trunk_count)?;
        self.main_file
            .seek(SeekFrom::End(0))
            .map_err(|error| format!("failed to restore file position: {error}"))?;
        Ok(())
    }
}

fn write_header(
    target: &mut File,
    compression_algorithm: CompressionAlgorithm,
    trunk_size_kb: u16,
    base_timestamp_millis: u64,
    total_log_line_count: u64,
    trunk_count: u32,
) -> Result<(), String> {
    target
        .write_all(&current_format_version()?)
        .map_err(|error| format!("failed to write format version: {error}"))?;
    target
        .write_all(&compression_algorithm.id().to_be_bytes())
        .map_err(|error| format!("failed to write compression algorithm: {error}"))?;
    target
        .write_all(&trunk_size_kb.to_be_bytes())
        .map_err(|error| format!("failed to write trunk size: {error}"))?;
    target
        .write_all(&base_timestamp_millis.to_be_bytes())
        .map_err(|error| format!("failed to write base timestamp: {error}"))?;
    target
        .write_all(&total_log_line_count.to_be_bytes())
        .map_err(|error| format!("failed to write total log line count: {error}"))?;
    write_u24(target, trunk_count)?;
    target
        .seek(SeekFrom::Start(HEADER_BYTES))
        .map_err(|error| format!("failed to finalize header: {error}"))?;
    Ok(())
}

fn print_conversion_summary(summary: &ConversionSummary, elapsed: Duration) {
    let compression_ratio = if summary.source_size_bytes == 0 {
        0.0
    } else {
        (summary.output_size_bytes as f64 / summary.source_size_bytes as f64) * 100.0
    };

    println!(
        "source size: {} ({})",
        summary.source_size_bytes,
        format_size(summary.source_size_bytes)
    );
    println!(
        "output size: {} ({})",
        summary.output_size_bytes,
        format_size(summary.output_size_bytes)
    );
    println!("compression ratio: {compression_ratio:.2}%");
    println!("elapsed: {}", format_duration(elapsed));
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }
    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.2} {}", UNITS[unit_index])
    }
}

fn format_duration(elapsed: Duration) -> String {
    if elapsed.as_secs() >= 1 {
        format!("{:.3}s", elapsed.as_secs_f64())
    } else if elapsed.as_millis() >= 1 {
        format!("{}ms", elapsed.as_millis())
    } else {
        format!("{}us", elapsed.as_micros())
    }
}

struct ProgressReporter<W: Write> {
    output: W,
    next_render_threshold: u64,
    last_rendered_lines: u64,
}

impl<W: Write> ProgressReporter<W> {
    fn new(output: W) -> Self {
        Self {
            output,
            next_render_threshold: 0,
            last_rendered_lines: 0,
        }
    }

    fn write_counting_message(&mut self, path: &Path) -> Result<(), String> {
        writeln!(self.output, "counting total lines in {}", path.display())
            .map_err(|error| format!("failed to write progress output: {error}"))
    }

    fn start(&mut self, total_lines: u64) -> Result<(), String> {
        self.next_render_threshold = PROGRESS_UPDATE_STEP;
        self.last_rendered_lines = 0;
        self.render(0, total_lines)
    }

    fn maybe_render(&mut self, processed_lines: u64, total_lines: u64) -> Result<(), String> {
        if processed_lines == total_lines || processed_lines >= self.next_render_threshold {
            self.render(processed_lines, total_lines)?;
            while self.next_render_threshold <= processed_lines {
                self.next_render_threshold = self
                    .next_render_threshold
                    .saturating_add(PROGRESS_UPDATE_STEP);
            }
        }
        Ok(())
    }

    fn finish(&mut self, total_lines: u64) -> Result<(), String> {
        if self.last_rendered_lines != total_lines {
            self.render(total_lines, total_lines)?;
        }
        writeln!(self.output).map_err(|error| format!("failed to write progress output: {error}"))
    }

    fn render(&mut self, processed_lines: u64, total_lines: u64) -> Result<(), String> {
        let percent = if total_lines == 0 {
            100.0
        } else {
            (processed_lines as f64 / total_lines as f64) * 100.0
        };
        write!(
            self.output,
            "\rprogress: {processed_lines}/{total_lines} ({percent:.2}%)"
        )
        .and_then(|_| self.output.flush())
        .map_err(|error| format!("failed to write progress output: {error}"))?;
        self.last_rendered_lines = processed_lines;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::format::{parse_bytes, ParsedLogEntry};

    use super::{
        convert_plaintext_log, format_duration, format_size, parse_level_and_message,
        validate_tinylog_path, CompressionAlgorithm, ConversionSummary, ProgressReporter,
    };

    #[test]
    fn convert_plaintext_log_writes_parseable_tog() {
        let input_path = unique_temp_path("plain.log");
        let output_path = unique_temp_path("plain.tog");
        fs::write(
            &input_path,
            concat!(
                "2026-05-01 22:01:00,253 [INFO] service started\n",
                "2026-05-01 22:01:00,278 [WARN] queue depth rising\n",
                "2026-05-01 22:01:00,353 [GID:] no level prefix\n"
            ),
        )
        .expect("write input log");
        let mut progress_output = Vec::new();
        let mut progress_reporter = ProgressReporter::new(&mut progress_output);

        let summary = convert_plaintext_log(
            &input_path,
            &output_path,
            CompressionAlgorithm::Gzip,
            512,
            &mut progress_reporter,
        )
        .expect("convert log");

        let bytes = fs::read(&output_path).expect("read output file");
        let entries = parse_bytes(&bytes).expect("parse output file");

        assert_eq!(
            entries,
            vec![
                ParsedLogEntry {
                    timestamp_millis: 1_777_672_860_253,
                    offset_millis: 0,
                    level: crate::format::LogLevel::Info,
                    content: "service started".to_string(),
                },
                ParsedLogEntry {
                    timestamp_millis: 1_777_672_860_278,
                    offset_millis: 25,
                    level: crate::format::LogLevel::Warn,
                    content: "queue depth rising".to_string(),
                },
                ParsedLogEntry {
                    timestamp_millis: 1_777_672_860_353,
                    offset_millis: 100,
                    level: crate::format::LogLevel::Info,
                    content: "[GID:] no level prefix".to_string(),
                },
            ]
        );
        let progress_text = String::from_utf8(progress_output).expect("utf8 progress");
        assert!(progress_text.contains("progress: 0/3"));
        assert!(progress_text.contains("progress: 3/3"));
        assert_eq!(
            summary,
            ConversionSummary {
                source_size_bytes: fs::metadata(&input_path).expect("input metadata").len(),
                output_size_bytes: fs::metadata(&output_path).expect("output metadata").len(),
            }
        );

        fs::remove_file(input_path).ok();
        fs::remove_file(output_path).ok();
    }

    #[test]
    fn parse_level_and_message_strips_supported_level_token() {
        let parsed = parse_level_and_message("2026-05-01 22:01:00,253 [FATAL] boom");

        assert_eq!(parsed.0, crate::format::LogLevel::Error);
        assert_eq!(parsed.1, "boom");
    }

    #[test]
    fn validate_tinylog_path_rejects_non_tog_outputs() {
        let error = validate_tinylog_path(Path::new("normal.log")).expect_err("extension error");

        assert_eq!(error, "TinyLog files must use the .tog extension");
    }

    #[test]
    fn format_size_renders_human_readable_values() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_536), "1.50 KiB");
        assert_eq!(format_size(1_048_576), "1.00 MiB");
    }

    #[test]
    fn format_duration_uses_stable_units() {
        assert_eq!(format_duration(Duration::from_micros(912)), "912us");
        assert_eq!(format_duration(Duration::from_millis(42)), "42ms");
        assert_eq!(format_duration(Duration::from_millis(1_250)), "1.250s");
    }

    fn unique_temp_path(file_name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("tinylog-viewer-{suffix}-{file_name}"))
    }
}
