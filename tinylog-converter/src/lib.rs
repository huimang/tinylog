use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{NaiveDateTime, TimeZone, Utc};

use tinylog_rust_common::format::{CompressionAlgorithm, LogLevel};

const FILE_EXTENSION: &str = ".tog";
const DEFAULT_TRUNK_SIZE_KB: u16 = 512;
const HEADER_BYTES: u64 = 26;
const BASE_TIMESTAMP_OFFSET: u64 = 7;
const TOTAL_LOG_LINE_COUNT_OFFSET: u64 = 15;
const PARALLEL_CONVERSION_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;
const MAX_OFFSET_MILLIS: u64 = 0xFFFF_FFFF;
const MAX_TRUNK_LOG_LINE_COUNT: u16 = 0xFFFF;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S,%3f";
const TIMESTAMP_TEXT_LENGTH: usize = 23;
const TIMESTAMP_SEPARATOR: char = ' ';
const LINE_HEADER_BYTES: usize = 9;
const BYTES_PER_KB: usize = 1024;
const INDEX_SCAN_BUFFER_BYTES: usize = 64 * 1024;
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

/// Describes one persisted trunk boundary before worker compression starts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedTrunk {
    trunk_index: usize,
    source_byte_start: u64,
    source_byte_length: u64,
}

/// Holds the complete conversion plan derived by the master thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConversionPlan {
    base_timestamp_millis: Option<u64>,
    trunks: Vec<PlannedTrunk>,
}

/// Groups a consecutive range of trunks for one worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedBatch {
    batch_index: usize,
    trunks: Vec<PlannedTrunk>,
}

/// Returns one worker output file that is ready for master-side merge.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BatchResult {
    batch_index: usize,
    temp_path: PathBuf,
    total_record_count: u64,
    first_timestamp_millis: Option<u64>,
    last_timestamp_millis: Option<u64>,
}

/// Reports one worker-side progress snapshot back to the master thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkerProgress {
    worker_index: usize,
    processed_trunks: usize,
    total_trunks: usize,
}

/// Carries worker progress and completion events back to the master thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkerMessage {
    Progress(WorkerProgress),
    Completed(BatchResult),
    Failed(String),
}

/// Holds one compressed trunk payload ready for final serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CompressedTrunk {
    line_count: u16,
    first_timestamp_millis: Option<u64>,
    last_timestamp_millis: Option<u64>,
    compressed_bytes: Vec<u8>,
}

/// Keeps worker temp files alive until the master merge completes.
#[derive(Debug)]
struct WorkerTempDirectory {
    path: PathBuf,
}

impl WorkerTempDirectory {
    fn new() -> Result<Self, String> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("failed to read system time: {error}"))?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tinylog-converter-workers-{suffix}"));
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        Ok(Self { path })
    }

    fn cleanup(self) -> Result<(), String> {
        fs::remove_dir_all(&self.path)
            .map_err(|error| format!("failed to remove {}: {error}", self.path.display()))
    }
}

/// Runs the Rust plaintext-to-TinyLog converter CLI.
pub fn run_convert_cli(arguments: &[String]) -> Result<(), String> {
    if arguments.len() < 2 || arguments.len() > 4 {
        return Err(
            "usage: tinylog-converter <input.log> <output.tog> [algorithmId] [trunkSizeKb]"
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
    if !should_use_parallel_conversion(source_size_bytes) {
        progress_reporter.write_phase_message(&format!(
            "using serial conversion mode for inputs up to {}",
            format_size(PARALLEL_CONVERSION_THRESHOLD_BYTES)
        ))?;
        progress_reporter.start(source_size_bytes)?;
        convert_plaintext_log_serial(
            plain_text_log_path,
            tinylog_path,
            compression_algorithm,
            trunk_size_kb,
            progress_reporter,
            source_size_bytes,
        )?;
        progress_reporter.finish(source_size_bytes)?;
        let output_size_bytes = file_size_bytes(tinylog_path)?;
        return Ok(ConversionSummary {
            source_size_bytes,
            output_size_bytes,
        });
    }

    progress_reporter.write_phase_message(&format!(
        "using parallel conversion mode for inputs larger than {}",
        format_size(PARALLEL_CONVERSION_THRESHOLD_BYTES)
    ))?;
    progress_reporter.write_phase_message(&format!(
        "building trunk index and preparing worker assignments for {}",
        plain_text_log_path.display()
    ))?;
    progress_reporter.start_indexing(source_size_bytes)?;
    let plan = build_conversion_plan(
        plain_text_log_path,
        trunk_size_kb,
        progress_reporter,
        source_size_bytes,
    )?;
    progress_reporter.finish_indexing(source_size_bytes)?;

    let worker_count = determine_worker_count(plan.trunks.len());
    let batches = build_worker_batches(&plan.trunks, worker_count);
    progress_reporter.write_phase_message(&format!(
        "compressing {} trunks with {} workers",
        plan.trunks.len(),
        batches.len()
    ))?;
    progress_reporter.start_parallel(0)?;
    let (temp_directory, batch_results) = run_worker_batches(
        plain_text_log_path,
        &plan,
        &batches,
        compression_algorithm,
        progress_reporter,
        0,
    )?;
    progress_reporter.finish_parallel(0)?;
    let merge_metadata = summarize_batch_results(&batch_results)?;

    merge_batch_results(
        tinylog_path,
        compression_algorithm,
        trunk_size_kb,
        plan.base_timestamp_millis
            .or(merge_metadata.first_timestamp_millis)
            .unwrap_or(0),
        merge_metadata.total_record_count,
        u32::try_from(plan.trunks.len())
            .map_err(|_| "trunk count must fit in 3 bytes".to_string())?,
        &batch_results,
    )?;
    temp_directory.cleanup()?;

    let output_size_bytes = file_size_bytes(tinylog_path)?;
    Ok(ConversionSummary {
        source_size_bytes,
        output_size_bytes,
    })
}

fn should_use_parallel_conversion(source_size_bytes: u64) -> bool {
    source_size_bytes > PARALLEL_CONVERSION_THRESHOLD_BYTES
}

fn validate_tinylog_path(tinylog_path: &Path) -> Result<(), String> {
    let file_name = tinylog_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid output path: {}", tinylog_path.display()))?;
    if !file_name.ends_with(FILE_EXTENSION) {
        return Err(format!(
            "TinyLog files must use the {FILE_EXTENSION} extension"
        ));
    }
    Ok(())
}

fn open_buffered_reader(path: &Path) -> Result<BufReader<File>, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
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
    if !is_record_start_line(line) {
        return Err(invalid_log_line_error(path, line_number));
    }
    Ok(())
}

fn invalid_log_line_error(path: &Path, line_number: usize) -> String {
    format!(
        "invalid log line at {}:{line_number}, expected '<yyyy-MM-dd HH:mm:ss,SSS> <message>'",
        path.display()
    )
}

fn is_record_start_line(line: &str) -> bool {
    is_record_start_prefix(line.as_bytes())
}

fn is_record_start_prefix(bytes: &[u8]) -> bool {
    if bytes.len() <= TIMESTAMP_TEXT_LENGTH || bytes[TIMESTAMP_TEXT_LENGTH] != b' ' {
        return false;
    }

    for (index, byte) in bytes[..TIMESTAMP_TEXT_LENGTH].iter().enumerate() {
        let expected = match index {
            4 | 7 => Some(b'-'),
            10 => Some(b' '),
            13 | 16 => Some(b':'),
            19 => Some(b','),
            _ => None,
        };
        if let Some(expected_byte) = expected {
            if *byte != expected_byte {
                return false;
            }
            continue;
        }
        if !byte.is_ascii_digit() {
            return false;
        }
    }

    true
}

fn append_continuation_line(record: &mut LogRecord, line: &str) {
    record.message.push('\n');
    record.message.push_str(line);
}

fn parse_timestamp_millis(path: &Path, line_number: usize, line: &str) -> Result<u64, String> {
    let date_time = NaiveDateTime::parse_from_str(&line[..TIMESTAMP_TEXT_LENGTH], TIMESTAMP_FORMAT)
        .map_err(|_| format!("invalid timestamp at {}:{line_number}", path.display()))?;
    let timestamp_millis = Utc.from_utc_datetime(&date_time).timestamp_millis();
    u64::try_from(timestamp_millis).map_err(|_| {
        format!(
            "timestamp before unix epoch at {}:{line_number}",
            path.display()
        )
    })
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

    for (index, segment) in version_without_suffix
        .split('.')
        .take(version.len())
        .enumerate()
    {
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

fn build_conversion_plan(
    path: &Path,
    trunk_size_kb: u16,
    progress_reporter: &mut ProgressReporter<impl Write>,
    source_size_bytes: u64,
) -> Result<ConversionPlan, String> {
    validate_trunk_size_kb(trunk_size_kb)?;
    let trunk_size_bytes =
        u64::try_from(usize::from(trunk_size_kb) * BYTES_PER_KB).unwrap_or(u64::MAX);
    let mut source_file =
        File::open(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut trunks = Vec::new();
    let Some(mut trunk_start) = find_next_record_start(&mut source_file, path, 0)? else {
        return Ok(ConversionPlan {
            base_timestamp_millis: None,
            trunks: Vec::new(),
        });
    };
    let base_timestamp_millis = read_first_record_timestamp(path, &mut source_file, trunk_start)?;
    progress_reporter
        .maybe_render_indexing(trunk_start.min(source_size_bytes), source_size_bytes)?;

    loop {
        let target_offset = trunk_start.saturating_add(trunk_size_bytes);
        let trunk_end = if target_offset >= source_size_bytes {
            source_size_bytes
        } else {
            find_next_record_start(&mut source_file, path, target_offset)?
                .unwrap_or(source_size_bytes)
        };

        trunks.push(PlannedTrunk {
            trunk_index: trunks.len(),
            source_byte_start: trunk_start,
            source_byte_length: trunk_end.saturating_sub(trunk_start),
        });
        progress_reporter
            .maybe_render_indexing(trunk_end.min(source_size_bytes), source_size_bytes)?;

        if trunk_end >= source_size_bytes {
            break;
        }
        trunk_start = trunk_end;
    }

    Ok(ConversionPlan {
        base_timestamp_millis,
        trunks,
    })
}

fn read_first_record_timestamp(
    path: &Path,
    source_file: &mut File,
    record_start_offset: u64,
) -> Result<Option<u64>, String> {
    source_file
        .seek(SeekFrom::Start(record_start_offset))
        .map_err(|error| format!("failed to seek {}: {error}", path.display()))?;
    let mut reader = BufReader::new(source_file);
    let mut line = String::new();
    let bytes_read = reader
        .read_line(&mut line)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes_read == 0 {
        return Ok(None);
    }
    trim_line_ending(&mut line);
    let record = parse_line(path, 1, &line)?;
    Ok(Some(record.timestamp_millis))
}

fn find_next_record_start(
    source_file: &mut File,
    path: &Path,
    start_offset: u64,
) -> Result<Option<u64>, String> {
    let file_size = source_file
        .metadata()
        .map_err(|error| format!("failed to read {} metadata: {error}", path.display()))?
        .len();
    if start_offset >= file_size {
        return Ok(None);
    }

    let mut at_line_start = start_offset == 0;
    if !at_line_start {
        source_file
            .seek(SeekFrom::Start(start_offset.saturating_sub(1)))
            .map_err(|error| format!("failed to seek {}: {error}", path.display()))?;
        let mut previous_byte = [0_u8; 1];
        source_file
            .read_exact(&mut previous_byte)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        at_line_start = previous_byte[0] == b'\n';
    }

    source_file
        .seek(SeekFrom::Start(start_offset))
        .map_err(|error| format!("failed to seek {}: {error}", path.display()))?;
    let mut buffer = [0_u8; INDEX_SCAN_BUFFER_BYTES];
    let mut absolute_offset = start_offset;
    let mut candidate_offset = None;
    let mut candidate_prefix = Vec::with_capacity(TIMESTAMP_TEXT_LENGTH + 1);

    loop {
        let bytes_read = source_file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if bytes_read == 0 {
            return Ok(None);
        }

        for byte in &buffer[..bytes_read] {
            if at_line_start {
                candidate_offset = Some(absolute_offset);
                candidate_prefix.clear();
                at_line_start = false;
            }

            if let Some(offset) = candidate_offset {
                if candidate_prefix.len() < TIMESTAMP_TEXT_LENGTH + 1 {
                    candidate_prefix.push(*byte);
                    if candidate_prefix.len() == TIMESTAMP_TEXT_LENGTH + 1
                        && is_record_start_prefix(&candidate_prefix)
                    {
                        return Ok(Some(offset));
                    }
                }
            }

            absolute_offset = absolute_offset.saturating_add(1);
            if *byte == b'\n' {
                at_line_start = true;
                candidate_offset = None;
                candidate_prefix.clear();
            }
        }
    }
}

fn write_trunk_record(
    source_path: &Path,
    raw_trunk_bytes: &mut Vec<u8>,
    base_timestamp_millis: u64,
    last_timestamp_millis: &mut Option<u64>,
    record_count: u16,
    record: LogRecord,
) -> Result<u16, String> {
    if let Some(last_timestamp) = *last_timestamp_millis {
        if record.timestamp_millis < last_timestamp {
            return Err(format!(
                "records must be appended in timestamp order in {}",
                source_path.display()
            ));
        }
    }
    write_raw_log_line(raw_trunk_bytes, &record, base_timestamp_millis)?;
    *last_timestamp_millis = Some(record.timestamp_millis);
    Ok(record_count.saturating_add(1))
}

fn determine_worker_count(trunk_count: usize) -> usize {
    if trunk_count == 0 {
        return 1;
    }
    let available = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    usize::min(available.max(1), trunk_count)
}

fn build_worker_batches(trunks: &[PlannedTrunk], worker_count: usize) -> Vec<PlannedBatch> {
    if trunks.is_empty() {
        return Vec::new();
    }

    let batch_count = usize::min(worker_count.max(1), trunks.len());
    let trunks_per_batch = trunks.len() / batch_count;
    let remainder = trunks.len() % batch_count;
    let mut cursor = 0usize;
    let mut batches = Vec::with_capacity(batch_count);

    for batch_index in 0..batch_count {
        let batch_len = trunks_per_batch + usize::from(batch_index < remainder);
        let batch_trunks = trunks[cursor..cursor + batch_len].to_vec();
        batches.push(PlannedBatch {
            batch_index,
            trunks: batch_trunks,
        });
        cursor += batch_len;
    }

    batches
}

fn run_worker_batches(
    source_path: &Path,
    plan: &ConversionPlan,
    batches: &[PlannedBatch],
    compression_algorithm: CompressionAlgorithm,
    progress_reporter: &mut ProgressReporter<impl Write>,
    _covered_source_lines: u64,
) -> Result<(WorkerTempDirectory, Vec<BatchResult>), String> {
    let temp_directory = WorkerTempDirectory::new()?;
    if batches.is_empty() {
        return Ok((temp_directory, Vec::new()));
    }

    let base_timestamp_millis = plan.base_timestamp_millis.unwrap_or(0);
    let (sender, receiver) = mpsc::channel::<WorkerMessage>();
    let mut handles = Vec::with_capacity(batches.len());
    let mut worker_states = vec![
        WorkerProgress {
            worker_index: 0,
            processed_trunks: 0,
            total_trunks: 0,
        };
        batches.len()
    ];
    for (index, batch) in batches.iter().enumerate() {
        worker_states[index] = WorkerProgress {
            worker_index: index,
            processed_trunks: 0,
            total_trunks: batch.trunks.len(),
        };
    }

    for batch in batches.iter().cloned() {
        let sender = sender.clone();
        let source_path = source_path.to_path_buf();
        let temp_dir = temp_directory.path.clone();
        let handle = thread::spawn(move || {
            if let Err(error) = process_batch(
                &source_path,
                &temp_dir,
                compression_algorithm,
                base_timestamp_millis,
                batch,
                &sender,
            ) {
                let _ = sender.send(WorkerMessage::Failed(error));
            }
        });
        handles.push(handle);
    }
    drop(sender);

    progress_reporter.render_worker_snapshot(&worker_states)?;

    let mut results = Vec::with_capacity(batches.len());
    while results.len() < batches.len() {
        match receiver
            .recv()
            .map_err(|error| format!("failed to receive worker result: {error}"))?
        {
            WorkerMessage::Progress(progress) => {
                if let Some(state) = worker_states.get_mut(progress.worker_index) {
                    *state = progress.clone();
                }
                progress_reporter.render_worker_snapshot(&worker_states)?;
            }
            WorkerMessage::Completed(batch_result) => {
                results.push(batch_result);
            }
            WorkerMessage::Failed(error) => return Err(error),
        }
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| "worker thread panicked during conversion".to_string())?;
    }

    results.sort_by_key(|batch_result| batch_result.batch_index);
    Ok((temp_directory, results))
}

fn process_batch(
    source_path: &Path,
    temp_dir: &Path,
    compression_algorithm: CompressionAlgorithm,
    base_timestamp_millis: u64,
    batch: PlannedBatch,
    sender: &mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    let mut source_file = File::open(source_path)
        .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;
    let temp_path = temp_dir.join(format!("batch-{:06}.part", batch.batch_index));
    let mut batch_file = File::create(&temp_path)
        .map_err(|error| format!("failed to create {}: {error}", temp_path.display()))?;
    let total_trunks = batch.trunks.len();
    let mut total_record_count = 0u64;
    let mut first_timestamp_millis = None;
    let mut last_timestamp_millis = None;

    for (processed_trunks, trunk) in batch.trunks.iter().enumerate() {
        let compressed_trunk = compress_planned_trunk(
            source_path,
            &mut source_file,
            trunk,
            compression_algorithm,
            base_timestamp_millis,
        )?;
        batch_file
            .write_all(&compressed_trunk.line_count.to_be_bytes())
            .map_err(|error| format!("failed to write {}: {error}", temp_path.display()))?;
        batch_file
            .write_all(
                &(u32::try_from(compressed_trunk.compressed_bytes.len())
                    .map_err(|_| "compressed trunk length must fit in 4 bytes".to_string())?)
                .to_be_bytes(),
            )
            .map_err(|error| format!("failed to write {}: {error}", temp_path.display()))?;
        batch_file
            .write_all(&compressed_trunk.compressed_bytes)
            .map_err(|error| format!("failed to write {}: {error}", temp_path.display()))?;
        total_record_count =
            total_record_count.saturating_add(u64::from(compressed_trunk.line_count));
        if first_timestamp_millis.is_none() {
            first_timestamp_millis = compressed_trunk.first_timestamp_millis;
        }
        last_timestamp_millis = compressed_trunk
            .last_timestamp_millis
            .or(last_timestamp_millis);
        sender
            .send(WorkerMessage::Progress(WorkerProgress {
                worker_index: batch.batch_index,
                processed_trunks: processed_trunks + 1,
                total_trunks,
            }))
            .map_err(|error| format!("failed to report worker progress: {error}"))?;
    }

    sender
        .send(WorkerMessage::Completed(BatchResult {
            batch_index: batch.batch_index,
            temp_path,
            total_record_count,
            first_timestamp_millis,
            last_timestamp_millis,
        }))
        .map_err(|error| format!("failed to report worker completion: {error}"))?;
    Ok(())
}

fn compress_planned_trunk(
    source_path: &Path,
    source_file: &mut File,
    trunk: &PlannedTrunk,
    compression_algorithm: CompressionAlgorithm,
    base_timestamp_millis: u64,
) -> Result<CompressedTrunk, String> {
    source_file
        .seek(SeekFrom::Start(trunk.source_byte_start))
        .map_err(|error| format!("failed to seek {}: {error}", source_path.display()))?;
    let mut source_bytes =
        vec![0_u8; usize::try_from(trunk.source_byte_length).unwrap_or(usize::MAX)];
    source_file
        .read_exact(&mut source_bytes)
        .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;

    let mut reader = BufReader::new(Cursor::new(source_bytes));
    let mut line = String::new();
    let mut line_number = 0usize;
    let mut record_count = 0u16;
    let mut last_timestamp_millis = None;
    let mut raw_trunk_bytes = Vec::new();
    let mut current_record = None;
    let mut first_timestamp_millis = None;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read {}: {error}", source_path.display()))?;
        if bytes_read == 0 {
            break;
        }
        line_number = line_number.saturating_add(1);
        trim_line_ending(&mut line);

        if line.trim().is_empty() {
            if let Some(record) = current_record.as_mut() {
                append_continuation_line(record, "");
            }
            continue;
        }

        if is_record_start_line(&line) {
            if let Some(record) = current_record.take() {
                if first_timestamp_millis.is_none() {
                    first_timestamp_millis = Some(record.timestamp_millis);
                }
                record_count = write_trunk_record(
                    source_path,
                    &mut raw_trunk_bytes,
                    base_timestamp_millis,
                    &mut last_timestamp_millis,
                    record_count,
                    record,
                )?;
            }
            current_record = Some(parse_line(
                source_path,
                usize::try_from(line_number).unwrap_or(usize::MAX),
                &line,
            )?);
            continue;
        }

        let Some(record) = current_record.as_mut() else {
            return Err(invalid_log_line_error(
                source_path,
                usize::try_from(line_number).unwrap_or(usize::MAX),
            ));
        };
        append_continuation_line(record, &line);
    }

    if let Some(record) = current_record.take() {
        if first_timestamp_millis.is_none() {
            first_timestamp_millis = Some(record.timestamp_millis);
        }
        record_count = write_trunk_record(
            source_path,
            &mut raw_trunk_bytes,
            base_timestamp_millis,
            &mut last_timestamp_millis,
            record_count,
            record,
        )?;
    }

    Ok(CompressedTrunk {
        line_count: record_count,
        first_timestamp_millis,
        last_timestamp_millis,
        compressed_bytes: compression_algorithm.compress(&raw_trunk_bytes)?,
    })
}

fn merge_batch_results(
    tinylog_path: &Path,
    compression_algorithm: CompressionAlgorithm,
    trunk_size_kb: u16,
    base_timestamp_millis: u64,
    total_log_line_count: u64,
    trunk_count: u32,
    batch_results: &[BatchResult],
) -> Result<(), String> {
    if let Some(parent) = tinylog_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut target = File::create(tinylog_path)
        .map_err(|error| format!("failed to create {}: {error}", tinylog_path.display()))?;
    write_header(
        &mut target,
        compression_algorithm,
        trunk_size_kb,
        base_timestamp_millis,
        total_log_line_count,
        trunk_count,
    )?;

    for batch_result in batch_results {
        let mut batch_file = File::open(&batch_result.temp_path).map_err(|error| {
            format!(
                "failed to read {}: {error}",
                batch_result.temp_path.display()
            )
        })?;
        io::copy(&mut batch_file, &mut target).map_err(|error| {
            format!(
                "failed to merge {}: {error}",
                batch_result.temp_path.display()
            )
        })?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BatchMergeMetadata {
    total_record_count: u64,
    first_timestamp_millis: Option<u64>,
}

fn summarize_batch_results(batch_results: &[BatchResult]) -> Result<BatchMergeMetadata, String> {
    let mut total_record_count = 0u64;
    let mut first_timestamp_millis = None;
    let mut last_timestamp_millis = None;

    for batch_result in batch_results {
        if let Some(first_timestamp) = batch_result.first_timestamp_millis {
            if let Some(last_timestamp) = last_timestamp_millis {
                if first_timestamp < last_timestamp {
                    return Err("records must be appended in timestamp order".to_string());
                }
            }
            if first_timestamp_millis.is_none() {
                first_timestamp_millis = Some(first_timestamp);
            }
        }
        total_record_count = total_record_count.saturating_add(batch_result.total_record_count);
        last_timestamp_millis = batch_result.last_timestamp_millis.or(last_timestamp_millis);
    }

    Ok(BatchMergeMetadata {
        total_record_count,
        first_timestamp_millis,
    })
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
    LINE_HEADER_BYTES + record.message.len()
}

fn write_raw_log_line(
    target: &mut Vec<u8>,
    record: &LogRecord,
    base_timestamp_millis: u64,
) -> Result<(), String> {
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
    fn new(
        path: &Path,
        compression_algorithm: CompressionAlgorithm,
        trunk_size_kb: u16,
    ) -> Result<Self, String> {
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
        write_header(
            &mut main_file,
            compression_algorithm,
            trunk_size_kb,
            0,
            0,
            0,
        )?;

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
        if self.current_trunk_line_count > 0
            && self.current_trunk_bytes + line_bytes > self.trunk_size_bytes
        {
            self.flush_current_trunk()?;
        }

        let base_timestamp_millis = self
            .base_timestamp_millis
            .unwrap_or(record.timestamp_millis);
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

fn convert_plaintext_log_serial(
    plain_text_log_path: &Path,
    tinylog_path: &Path,
    compression_algorithm: CompressionAlgorithm,
    trunk_size_kb: u16,
    progress_reporter: &mut ProgressReporter<impl Write>,
    source_size_bytes: u64,
) -> Result<(), String> {
    let mut reader = open_buffered_reader(plain_text_log_path)?;
    let mut writer = TinylogWriter::new(tinylog_path, compression_algorithm, trunk_size_kb)?;
    let mut line = String::new();
    let mut line_number = 0usize;
    let mut processed_bytes = 0u64;
    let mut current_record = None;

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            format!("failed to read {}: {error}", plain_text_log_path.display())
        })?;
        if bytes_read == 0 {
            break;
        }
        trim_line_ending(&mut line);
        line_number += 1;
        processed_bytes = processed_bytes.saturating_add(bytes_read as u64);

        if line.trim().is_empty() {
            if let Some(record) = current_record.as_mut() {
                append_continuation_line(record, "");
            }
            progress_reporter.maybe_render(processed_bytes, source_size_bytes)?;
            continue;
        }

        if is_record_start_line(&line) {
            if let Some(record) = current_record.take() {
                writer.append(record)?;
            }
            current_record = Some(parse_line(plain_text_log_path, line_number, &line)?);
            progress_reporter.maybe_render(processed_bytes, source_size_bytes)?;
            continue;
        }

        let Some(record) = current_record.as_mut() else {
            return Err(invalid_log_line_error(plain_text_log_path, line_number));
        };
        append_continuation_line(record, &line);
        progress_reporter.maybe_render(processed_bytes, source_size_bytes)?;
    }

    if let Some(record) = current_record.take() {
        writer.append(record)?;
    }

    writer.close()
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

    fn write_phase_message(&mut self, message: &str) -> Result<(), String> {
        writeln!(self.output, "{message}")
            .map_err(|error| format!("failed to write progress output: {error}"))
    }

    fn render_worker_snapshot(&mut self, workers: &[WorkerProgress]) -> Result<(), String> {
        let mut line = String::from("writing:");
        for worker in workers {
            let percent = if worker.total_trunks == 0 {
                100
            } else {
                worker.processed_trunks.saturating_mul(100) / worker.total_trunks
            };
            line.push_str(&format!(" {}: {}%", worker.worker_index + 1, percent));
        }
        write!(self.output, "\r{line}\x1b[K")
            .and_then(|_| self.output.flush())
            .map_err(|error| format!("failed to write progress output: {error}"))
    }

    fn start(&mut self, total_lines: u64) -> Result<(), String> {
        self.next_render_threshold = PROGRESS_UPDATE_STEP;
        self.last_rendered_lines = 0;
        self.render(0, total_lines)
    }

    fn start_parallel(&mut self, total_lines: u64) -> Result<(), String> {
        self.next_render_threshold = PROGRESS_UPDATE_STEP;
        self.last_rendered_lines = 0;
        let _ = total_lines;
        Ok(())
    }

    fn start_indexing(&mut self, total_lines: u64) -> Result<(), String> {
        self.next_render_threshold = PROGRESS_UPDATE_STEP;
        self.last_rendered_lines = 0;
        self.render_indexing(0, total_lines)
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

    fn maybe_render_indexing(
        &mut self,
        processed_lines: u64,
        total_lines: u64,
    ) -> Result<(), String> {
        if processed_lines == total_lines || processed_lines >= self.next_render_threshold {
            self.render_indexing(processed_lines, total_lines)?;
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

    fn finish_parallel(&mut self, total_lines: u64) -> Result<(), String> {
        self.last_rendered_lines = total_lines;
        writeln!(self.output).map_err(|error| format!("failed to write progress output: {error}"))
    }

    fn finish_indexing(&mut self, total_lines: u64) -> Result<(), String> {
        if self.last_rendered_lines != total_lines {
            self.render_indexing(total_lines, total_lines)?;
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

    fn render_indexing(&mut self, processed_lines: u64, total_lines: u64) -> Result<(), String> {
        let percent = if total_lines == 0 {
            100.0
        } else {
            (processed_lines as f64 / total_lines as f64) * 100.0
        };
        write!(
            self.output,
            "\rindexing: {processed_lines}/{total_lines} ({percent:.2}%)"
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

    use tinylog_rust_common::format::{parse_bytes, LogLevel, ParsedLogEntry};

    use super::{
        build_conversion_plan, build_worker_batches, convert_plaintext_log, format_duration,
        format_size, parse_level_and_message, should_use_parallel_conversion,
        validate_tinylog_path, CompressionAlgorithm, ConversionSummary, ProgressReporter,
        PARALLEL_CONVERSION_THRESHOLD_BYTES,
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
                    level: LogLevel::Info,
                    content: "service started".to_string(),
                },
                ParsedLogEntry {
                    timestamp_millis: 1_777_672_860_278,
                    offset_millis: 25,
                    level: LogLevel::Warn,
                    content: "queue depth rising".to_string(),
                },
                ParsedLogEntry {
                    timestamp_millis: 1_777_672_860_353,
                    offset_millis: 100,
                    level: LogLevel::Info,
                    content: "[GID:] no level prefix".to_string(),
                },
            ]
        );
        let progress_text = String::from_utf8(progress_output).expect("utf8 progress");
        let source_size_bytes = fs::metadata(&input_path).expect("input metadata").len();
        assert!(progress_text.contains("using serial conversion mode"));
        assert!(progress_text.contains(&format!("progress: 0/{source_size_bytes}")));
        assert!(progress_text.contains(&format!(
            "progress: {source_size_bytes}/{source_size_bytes}"
        )));
        assert_eq!(
            summary,
            ConversionSummary {
                source_size_bytes,
                output_size_bytes: fs::metadata(&output_path).expect("output metadata").len(),
            }
        );

        fs::remove_file(input_path).ok();
        fs::remove_file(output_path).ok();
    }

    #[test]
    fn build_conversion_plan_splits_consecutive_trunks() {
        let input_path = unique_temp_path("planned.log");
        fs::write(
            &input_path,
            concat!(
                "2026-05-01 22:01:00,253 [INFO] alpha\n",
                "2026-05-01 22:01:00,278 [WARN] beta\n",
                "\n",
                "2026-05-01 22:01:00,353 [ERROR] gamma\n"
            ),
        )
        .expect("write planning input");
        let mut progress_output = Vec::new();
        let mut progress_reporter = ProgressReporter::new(&mut progress_output);

        let source_size_bytes = fs::metadata(&input_path).expect("input metadata").len();
        let plan = build_conversion_plan(&input_path, 1, &mut progress_reporter, source_size_bytes)
            .expect("build plan");

        assert_eq!(plan.base_timestamp_millis, Some(1_777_672_860_253));
        assert_eq!(plan.trunks.len(), 1);
        assert_eq!(plan.trunks[0].source_byte_start, 0);
        assert_eq!(plan.trunks[0].source_byte_length, source_size_bytes);

        fs::remove_file(input_path).ok();
    }

    #[test]
    fn build_conversion_plan_aligns_boundary_to_next_record_start() {
        let input_path = unique_temp_path("planned-multiline.log");
        let mut input = String::from("2026-05-01 22:01:00,253 [INFO] alpha\n");
        for index in 0..80 {
            input.push_str(&format!(
                "stack frame {index:02} detail detail detail detail detail detail\n"
            ));
        }
        input.push_str("2026-05-01 22:01:01,253 [WARN] omega\n");
        fs::write(&input_path, input).expect("write planning input");

        let source_size_bytes = fs::metadata(&input_path).expect("input metadata").len();
        let mut progress_output = Vec::new();
        let mut progress_reporter = ProgressReporter::new(&mut progress_output);
        let plan = build_conversion_plan(&input_path, 1, &mut progress_reporter, source_size_bytes)
            .expect("build plan");

        assert_eq!(plan.base_timestamp_millis, Some(1_777_672_860_253));
        assert_eq!(plan.trunks.len(), 2);
        assert!(plan.trunks[0].source_byte_length > 1024);
        assert_eq!(
            plan.trunks[1].source_byte_start,
            plan.trunks[0].source_byte_start + plan.trunks[0].source_byte_length
        );

        fs::remove_file(input_path).ok();
    }

    #[test]
    fn build_worker_batches_preserves_contiguous_trunk_order() {
        let trunks = vec![
            planned_trunk(0, 0, 10),
            planned_trunk(1, 10, 10),
            planned_trunk(2, 20, 10),
            planned_trunk(3, 30, 10),
            planned_trunk(4, 40, 10),
        ];

        let batches = build_worker_batches(&trunks, 2);

        assert_eq!(batches.len(), 2);
        assert_eq!(
            batches[0]
                .trunks
                .iter()
                .map(|trunk| trunk.trunk_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            batches[1]
                .trunks
                .iter()
                .map(|trunk| trunk.trunk_index)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
    }

    #[test]
    fn should_use_parallel_conversion_only_above_threshold() {
        assert!(!should_use_parallel_conversion(
            PARALLEL_CONVERSION_THRESHOLD_BYTES
        ));
        assert!(should_use_parallel_conversion(
            PARALLEL_CONVERSION_THRESHOLD_BYTES.saturating_add(1)
        ));
    }

    #[test]
    fn parse_level_and_message_strips_supported_level_token() {
        let parsed = parse_level_and_message("2026-05-01 22:01:00,253 [FATAL] boom");

        assert_eq!(parsed.0, LogLevel::Error);
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

    #[test]
    fn convert_plaintext_log_preserves_multiline_records() {
        let input_path = unique_temp_path("plain-multiline.log");
        let output_path = unique_temp_path("plain-multiline.tog");
        fs::write(
            &input_path,
            concat!(
                "2026-05-01 22:01:00,253 [ERROR] request failed\n",
                "java.lang.IllegalStateException: boom\n",
                "\tat example.Service.handle(Service.java:42)\n",
                "2026-05-01 22:01:00,278 [INFO] recovered\n"
            ),
        )
        .expect("write multiline input log");
        let mut progress_output = Vec::new();
        let mut progress_reporter = ProgressReporter::new(&mut progress_output);

        convert_plaintext_log(
            &input_path,
            &output_path,
            CompressionAlgorithm::Gzip,
            512,
            &mut progress_reporter,
        )
        .expect("convert multiline log");

        let bytes = fs::read(&output_path).expect("read output file");
        let entries = parse_bytes(&bytes).expect("parse output file");

        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].content,
            "request failed\njava.lang.IllegalStateException: boom\n\tat example.Service.handle(Service.java:42)"
        );
        assert_eq!(entries[1].content, "recovered");

        fs::remove_file(input_path).ok();
        fs::remove_file(output_path).ok();
    }

    fn planned_trunk(
        trunk_index: usize,
        source_byte_start: u64,
        source_byte_length: u64,
    ) -> super::PlannedTrunk {
        super::PlannedTrunk {
            trunk_index,
            source_byte_start,
            source_byte_length,
        }
    }

    fn unique_temp_path(file_name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("tinylog-converter-{suffix}-{file_name}"))
    }
}
