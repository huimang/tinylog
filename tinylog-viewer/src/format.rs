use std::{
    fs,
    io::{BufReader, Cursor, Read, Seek, SeekFrom, Write},
};

use bzip2::{read::BzDecoder, write::BzEncoder, Compression as BzCompression};
use chrono::{TimeZone, Utc};
use flate2::{
    read::{DeflateDecoder, GzDecoder},
    write::{DeflateEncoder, GzEncoder},
    Compression as FlateCompression,
};
use snap::read::FrameDecoder;
use snap::write::FrameEncoder;
use zip::{write::SimpleFileOptions, ZipWriter};
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use zip::ZipArchive;

/// Holds the total record count together with the currently visible entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleLogWindow {
    pub total_records: u64,
    pub visible_entries: Vec<ParsedLogEntry>,
}

/// Represents one rendered log entry parsed from the trunk-based TinyLog format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLogEntry {
    pub timestamp_millis: u64,
    pub offset_millis: u32,
    pub level: LogLevel,
    pub content: String,
}

/// Represents the persisted one-byte log level in trunk lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Holds the parsed file header needed by the viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileHeader {
    compression_algorithm: CompressionAlgorithm,
    base_timestamp_millis: u64,
    total_records: u64,
    trunk_count: u32,
}

/// Caches the persisted trunk metadata needed for fast window reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TinylogFileIndex {
    path: String,
    header: FileHeader,
    trunks: Vec<TrunkLocation>,
}

/// Stores one persisted trunk offset together with its logical record range.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TrunkLocation {
    start_offset: u64,
    record_start_index: usize,
    line_count: usize,
    compressed_content_length: usize,
}

#[allow(dead_code)]
impl TinylogFileIndex {
    /// Returns the total persisted logical record count.
    pub(crate) fn total_records(&self) -> u64 {
        self.header.total_records
    }

    /// Returns the total persisted trunk count.
    pub(crate) fn trunk_count(&self) -> usize {
        self.trunks.len()
    }

    /// Returns the 1-based trunk position that owns the provided logical record index.
    pub(crate) fn trunk_position_for_record(&self, record_index: usize) -> Option<usize> {
        self.trunks.iter().enumerate().find_map(|(index, trunk)| {
            let trunk_end = trunk.record_start_index.saturating_add(trunk.line_count);
            if record_index >= trunk.record_start_index && record_index < trunk_end {
                Some(index.saturating_add(1))
            } else {
                None
            }
        })
    }

    /// Returns the logical record start index for one trunk.
    pub(crate) fn trunk_record_start(&self, trunk_index: usize) -> Option<usize> {
        self.trunks.get(trunk_index).map(|trunk| trunk.record_start_index)
    }

    /// Returns the zero-based trunk index that owns the provided logical record index.
    pub(crate) fn trunk_index_for_record(&self, record_index: usize) -> Option<usize> {
        self.trunks.iter().position(|trunk| {
            let trunk_end = trunk.record_start_index.saturating_add(trunk.line_count);
            record_index >= trunk.record_start_index && record_index < trunk_end
        })
    }
}

/// Enumerates the supported trunk-level compression algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    None,
    Gzip,
    Zip,
    Deflate,
    Bzip2,
    Xz,
    Zstd,
    Snappy,
}

/// Reads and parses one trunk-based TinyLog file from disk.
#[allow(dead_code)]
pub fn read_file(path: &str) -> Result<Vec<ParsedLogEntry>, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    parse_bytes(&bytes)
}

/// Reads only the currently visible window from one trunk-based TinyLog file.
#[allow(dead_code)]
pub fn read_visible_window(
    path: &str,
    start_index: usize,
    visible_count: usize,
) -> Result<VisibleLogWindow, String> {
    let index = scan_file_index(path)?;
    read_visible_window_from_index(&index, start_index, visible_count)
}

/// Reads the final visible window from the cached in-memory trunk index.
#[allow(dead_code)]
pub fn read_last_window(path: &str, visible_count: usize) -> Result<VisibleLogWindow, String> {
    let index = scan_file_index(path)?;
    read_last_window_from_index(&index, visible_count)
}

/// Formats persisted UTC milliseconds as the human-readable normal log timestamp shape.
pub fn format_timestamp_millis(timestamp_millis: u64) -> Result<String, String> {
    let timestamp_millis = i64::try_from(timestamp_millis)
        .map_err(|_| "timestamp exceeds supported range".to_string())?;
    let date_time = Utc
        .timestamp_millis_opt(timestamp_millis)
        .single()
        .ok_or_else(|| "timestamp cannot be represented in UTC".to_string())?;
    Ok(date_time.format("%Y-%m-%d %H:%M:%S,%3f").to_string())
}

/// Parses bytes using the current trunk-based layout.
#[allow(dead_code)]
pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<ParsedLogEntry>, String> {
    let mut cursor = CursorReader::new(bytes);
    let header = read_header_from_cursor(&mut cursor)?;
    let mut entries = Vec::new();

    for _ in 0..header.trunk_count {
        let trunk_log_line_count = usize::from(cursor.read_u16()?);
        let compressed_content_length = cursor.read_u32()? as usize;
        let compressed_content = cursor.read_exact(compressed_content_length)?.to_vec();
        let raw_trunk_bytes = header
            .compression_algorithm
            .decompress(compressed_content)?;
        entries.extend(parse_raw_trunk_payload(
            &raw_trunk_bytes,
            header.base_timestamp_millis,
            trunk_log_line_count,
        )?);
    }

    if entries.len() != usize::try_from(header.total_records).unwrap_or(usize::MAX) {
        return Err("header total record count does not match parsed trunk entries".to_string());
    }
    if cursor.remaining() != 0 {
        return Err("unexpected trailing bytes after parsing prototype file".to_string());
    }

    Ok(entries)
}

/// Parses the fixed header from one stream reader.
fn read_header_from_reader(reader: &mut impl Read) -> Result<FileHeader, String> {
    let mut version = [0_u8; 3];
    reader
        .read_exact(&mut version)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    let compression_algorithm = CompressionAlgorithm::from_id(read_u16_from_reader(reader)?)?;
    validate_trunk_size_kb(read_u16_from_reader(reader)?)?;
    let base_timestamp_millis = read_u64_from_reader(reader)?;
    let total_records = read_u64_from_reader(reader)?;
    let trunk_count = read_u24_from_reader(reader)?;
    Ok(FileHeader {
        compression_algorithm,
        base_timestamp_millis,
        total_records,
        trunk_count,
    })
}

/// Parses the fixed header from a deterministic byte cursor.
fn read_header_from_cursor(cursor: &mut CursorReader<'_>) -> Result<FileHeader, String> {
    cursor.read_exact(3)?;
    let compression_algorithm = CompressionAlgorithm::from_id(cursor.read_u16()?)?;
    validate_trunk_size_kb(cursor.read_u16()?)?;
    let base_timestamp_millis = cursor.read_u64()?;
    let total_records = cursor.read_u64()?;
    let trunk_count = cursor.read_u24()?;
    Ok(FileHeader {
        compression_algorithm,
        base_timestamp_millis,
        total_records,
        trunk_count,
    })
}

/// Builds the in-memory trunk index by scanning trunk headers once without decompressing payloads.
pub(crate) fn scan_file_index(path: &str) -> Result<TinylogFileIndex, String> {
    let file = fs::File::open(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    let mut reader = BufReader::new(file);
    let header = read_header_from_reader(&mut reader)?;
    let mut trunks = Vec::with_capacity(usize::try_from(header.trunk_count).unwrap_or(0));
    let mut record_start_index = 0usize;
    let mut current_offset = read_header_size();

    for _ in 0..header.trunk_count {
        let trunk_log_line_count = usize::from(read_u16_from_reader(&mut reader)?);
        let compressed_content_length = read_u32_from_reader(&mut reader)? as usize;
        trunks.push(TrunkLocation {
            start_offset: current_offset,
            record_start_index,
            line_count: trunk_log_line_count,
            compressed_content_length,
        });
        skip_exact(&mut reader, compressed_content_length)?;
        current_offset = current_offset
            .saturating_add(2)
            .saturating_add(4)
            .saturating_add(u64::try_from(compressed_content_length).unwrap_or(u64::MAX));
        record_start_index = record_start_index.saturating_add(trunk_log_line_count);
    }

    if record_start_index != usize::try_from(header.total_records).unwrap_or(usize::MAX) {
        return Err("header total record count does not match scanned trunk index".to_string());
    }

    Ok(TinylogFileIndex {
        path: path.to_string(),
        header,
        trunks,
    })
}

/// Reads one visible window from a previously scanned trunk index.
pub(crate) fn read_visible_window_from_index(
    index: &TinylogFileIndex,
    start_index: usize,
    visible_count: usize,
) -> Result<VisibleLogWindow, String> {
    let total_records_usize = usize::try_from(index.header.total_records).unwrap_or(usize::MAX);
    let start_index = usize::min(start_index, total_records_usize);
    let end_index = usize::min(start_index.saturating_add(visible_count), total_records_usize);
    if start_index >= end_index {
        return Ok(VisibleLogWindow {
            total_records: index.header.total_records,
            visible_entries: Vec::new(),
        });
    }

    let mut file = fs::File::open(&index.path)
        .map_err(|error| format!("failed to read {}: {error}", index.path))?;
    let mut visible_entries = Vec::with_capacity(end_index.saturating_sub(start_index));

    for trunk in &index.trunks {
        let trunk_start = trunk.record_start_index;
        let trunk_end = trunk.record_start_index.saturating_add(trunk.line_count);
        if trunk_end <= start_index {
            continue;
        }
        if trunk_start >= end_index {
            break;
        }

        let trunk_entries = read_trunk_at(&mut file, &index.header, trunk)?;
        let local_start = start_index.saturating_sub(trunk_start);
        let local_end = usize::min(trunk_entries.len(), end_index.saturating_sub(trunk_start));
        visible_entries.extend(
            trunk_entries
                .into_iter()
                .skip(local_start)
                .take(local_end.saturating_sub(local_start)),
        );
    }

    Ok(VisibleLogWindow {
        total_records: index.header.total_records,
        visible_entries,
    })
}

/// Reads the final visible window from a previously scanned trunk index.
pub(crate) fn read_last_window_from_index(
    index: &TinylogFileIndex,
    visible_count: usize,
) -> Result<VisibleLogWindow, String> {
    let total_records = usize::try_from(index.header.total_records).unwrap_or(usize::MAX);
    let start_index = total_records.saturating_sub(visible_count);
    read_visible_window_from_index(index, start_index, visible_count)
}

/// Reads and parses one cached trunk by its zero-based index.
#[allow(dead_code)]
pub(crate) fn read_trunk_entries(
    index: &TinylogFileIndex,
    trunk_index: usize,
) -> Result<Vec<ParsedLogEntry>, String> {
    let trunk = index
        .trunks
        .get(trunk_index)
        .ok_or_else(|| format!("invalid trunk index: {trunk_index}"))?;
    let mut file = fs::File::open(&index.path)
        .map_err(|error| format!("failed to read {}: {error}", index.path))?;
    read_trunk_at(&mut file, &index.header, trunk)
}

/// Scans trunks in the provided order and exposes their decompressed logical entries.
#[allow(dead_code)]
pub(crate) fn scan_trunks_in_order<V>(
    index: &TinylogFileIndex,
    trunk_order: &[usize],
    mut visit_trunk: V,
) -> Result<(), String>
where
    V: FnMut(usize, &[ParsedLogEntry]) -> Result<(), String>,
{
    if trunk_order.is_empty() {
        return Ok(());
    }

    let mut file = fs::File::open(&index.path)
        .map_err(|error| format!("failed to read {}: {error}", index.path))?;
    for trunk_index in trunk_order {
        let trunk = index
            .trunks
            .get(*trunk_index)
            .ok_or_else(|| format!("invalid trunk index: {trunk_index}"))?;
        let trunk_entries = read_trunk_at(&mut file, &index.header, trunk)?;
        visit_trunk(*trunk_index, &trunk_entries)?;
    }

    Ok(())
}

/// Visits one logical range inside a decompressed trunk.
#[allow(dead_code)]
fn visit_entries_in_range<F>(
    record_start_index: usize,
    entries: &[ParsedLogEntry],
    local_start: usize,
    local_end: usize,
    visit_entry: &mut F,
) -> Result<(), String>
where
    F: FnMut(usize, &ParsedLogEntry) -> Result<(), String>,
{
    for (offset, entry) in entries
        .iter()
        .enumerate()
        .skip(local_start)
        .take(local_end.saturating_sub(local_start))
    {
        let logical_index = record_start_index.saturating_add(offset);
        visit_entry(logical_index, entry)?;
    }
    Ok(())
}

/// Reads and parses one persisted trunk from a known byte offset.
fn read_trunk_at(
    file: &mut fs::File,
    header: &FileHeader,
    trunk: &TrunkLocation,
) -> Result<Vec<ParsedLogEntry>, String> {
    file.seek(SeekFrom::Start(trunk.start_offset))
        .map_err(|error| format!("failed to seek to trunk start: {error}"))?;
    let trunk_log_line_count = usize::from(read_u16_from_reader(file)?);
    let compressed_content_length = read_u32_from_reader(file)? as usize;
    if compressed_content_length != trunk.compressed_content_length {
        return Err("cached trunk index does not match persisted trunk length".to_string());
    }
    let mut compressed_content = vec![0_u8; compressed_content_length];
    file.read_exact(&mut compressed_content)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    let raw_trunk_bytes = header
        .compression_algorithm
        .decompress(compressed_content)?;
    parse_raw_trunk_payload(
        &raw_trunk_bytes,
        header.base_timestamp_millis,
        trunk_log_line_count,
    )
}

/// Parses one decompressed trunk payload into logical entries.
fn parse_raw_trunk_payload(
    raw_trunk_bytes: &[u8],
    base_timestamp_millis: u64,
    trunk_log_line_count: usize,
) -> Result<Vec<ParsedLogEntry>, String> {
    let mut cursor = CursorReader::new(raw_trunk_bytes);
    let mut entries = Vec::with_capacity(trunk_log_line_count);
    for _ in 0..trunk_log_line_count {
        let offset_millis = cursor.read_u32()?;
        let level = LogLevel::from_id(cursor.read_u8()?)?;
        let content_length = cursor.read_u32()? as usize;
        let content_bytes = cursor.read_exact(content_length)?;
        let content = String::from_utf8(content_bytes.to_vec())
            .map_err(|error| format!("invalid utf-8 log content: {error}"))?;
        entries.push(ParsedLogEntry {
            timestamp_millis: base_timestamp_millis + u64::from(offset_millis),
            offset_millis,
            level,
            content,
        });
    }
    if cursor.remaining() != 0 {
        return Err("raw trunk payload contains trailing bytes".to_string());
    }
    Ok(entries)
}

impl LogLevel {
    /// Resolves one persisted one-byte level identifier.
    pub(crate) fn from_id(id: u8) -> Result<Self, String> {
        match id {
            0 => Ok(Self::Trace),
            1 => Ok(Self::Debug),
            2 => Ok(Self::Info),
            3 => Ok(Self::Warn),
            4 => Ok(Self::Error),
            _ => Err(format!("unsupported persisted log level id: {id}")),
        }
    }

    /// Returns the bracketed text label shown by the viewer.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Trace => "[TRACE]",
            Self::Debug => "[DEBUG]",
            Self::Info => "[INFO]",
            Self::Warn => "[WARN]",
            Self::Error => "[ERROR]",
        }
    }

    /// Returns the lowercase name used by viewer commands.
    #[allow(dead_code)]
    pub(crate) fn command_name(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Parses one plaintext token into the structured level.
    pub(crate) fn parse_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_uppercase().as_str() {
            "TRACE" => Some(Self::Trace),
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            "FATAL" => Some(Self::Error),
            _ => None,
        }
    }

    /// Returns the persisted one-byte identifier.
    pub(crate) fn to_persisted_id(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warn => 3,
            Self::Error => 4,
        }
    }
}

impl CompressionAlgorithm {
    /// Resolves one persisted algorithm identifier.
    pub(crate) fn from_id(id: u16) -> Result<Self, String> {
        match id {
            0 => Ok(Self::None),
            1 => Ok(Self::Gzip),
            2 => Ok(Self::Zip),
            3 => Ok(Self::Deflate),
            4 => Ok(Self::Bzip2),
            5 => Ok(Self::Xz),
            6 => Ok(Self::Zstd),
            7 => Ok(Self::Snappy),
            _ => Err(format!("unsupported compression algorithm id: {id}")),
        }
    }

    /// Returns the persisted two-byte algorithm identifier.
    pub(crate) fn id(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Gzip => 1,
            Self::Zip => 2,
            Self::Deflate => 3,
            Self::Bzip2 => 4,
            Self::Xz => 5,
            Self::Zstd => 6,
            Self::Snappy => 7,
        }
    }

    /// Returns the stable display name used by the CLI.
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Zip => "zip",
            Self::Deflate => "deflate",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Zstd => "zstd",
            Self::Snappy => "snappy",
        }
    }

    /// Decompresses one trunk payload according to the selected header algorithm.
    fn decompress(self, payload: Vec<u8>) -> Result<Vec<u8>, String> {
        match self {
            Self::None => Ok(payload),
            Self::Gzip => read_all_from_decoder(GzDecoder::new(Cursor::new(payload))),
            Self::Zip => {
                let cursor = Cursor::new(payload);
                let mut archive = ZipArchive::new(cursor)
                    .map_err(|error| format!("invalid zip payload: {error}"))?;
                if archive.is_empty() {
                    return Err("zip payload does not contain an entry".to_string());
                }
                let mut entry = archive
                    .by_index(0)
                    .map_err(|error| format!("failed to open zip entry: {error}"))?;
                read_all_from_decoder(&mut entry)
            }
            Self::Deflate => read_all_from_decoder(DeflateDecoder::new(Cursor::new(payload))),
            Self::Bzip2 => read_all_from_decoder(BzDecoder::new(Cursor::new(payload))),
            Self::Xz => read_all_from_decoder(XzDecoder::new(Cursor::new(payload))),
            Self::Zstd => read_all_from_decoder(
                zstd::stream::read::Decoder::new(Cursor::new(payload))
                    .map_err(|error| format!("invalid zstd payload: {error}"))?,
            ),
            Self::Snappy => read_all_from_decoder(FrameDecoder::new(Cursor::new(payload))),
        }
    }

    /// Compresses one raw trunk payload according to the selected header algorithm.
    pub(crate) fn compress(self, payload: &[u8]) -> Result<Vec<u8>, String> {
        match self {
            Self::None => Ok(payload.to_vec()),
            Self::Gzip => {
                let mut encoder = GzEncoder::new(Vec::new(), FlateCompression::default());
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to compress payload: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("failed to finish gzip payload: {error}"))
            }
            Self::Zip => {
                let mut encoder = ZipWriter::new(Cursor::new(Vec::new()));
                encoder
                    .start_file("payload", SimpleFileOptions::default())
                    .map_err(|error| format!("failed to start zip payload: {error}"))?;
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to write zip payload: {error}"))?;
                let cursor = encoder
                    .finish()
                    .map_err(|error| format!("failed to finish zip payload: {error}"))?;
                Ok(cursor.into_inner())
            }
            Self::Deflate => {
                let mut encoder = DeflateEncoder::new(Vec::new(), FlateCompression::default());
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to compress payload: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("failed to finish deflate payload: {error}"))
            }
            Self::Bzip2 => {
                let mut encoder = BzEncoder::new(Vec::new(), BzCompression::default());
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to compress payload: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("failed to finish bzip2 payload: {error}"))
            }
            Self::Xz => {
                let mut encoder = XzEncoder::new(Vec::new(), 6);
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to compress payload: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("failed to finish xz payload: {error}"))
            }
            Self::Zstd => {
                let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0)
                    .map_err(|error| format!("failed to create zstd encoder: {error}"))?;
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to write zstd payload: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("failed to finish zstd payload: {error}"))
            }
            Self::Snappy => {
                let mut encoder = FrameEncoder::new(Vec::new());
                encoder
                    .write_all(payload)
                    .map_err(|error| format!("failed to compress payload: {error}"))?;
                encoder
                    .into_inner()
                    .map_err(|error| format!("failed to finish snappy payload: {error}"))
            }
        }
    }
}

/// Validates one configured trunk size in KB.
fn validate_trunk_size_kb(trunk_size_kb: u16) -> Result<u16, String> {
    if trunk_size_kb == 0 {
        return Err("trunk size must be greater than zero".to_string());
    }
    Ok(trunk_size_kb)
}

/// Returns the fixed file header size for the current on-disk layout.
fn read_header_size() -> u64 {
    26
}

/// Reads one big-endian 16-bit integer from a stream.
fn read_u16_from_reader(reader: &mut impl Read) -> Result<u16, String> {
    let mut bytes = [0_u8; 2];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    Ok(u16::from_be_bytes(bytes))
}

/// Reads one big-endian 64-bit integer from a stream.
fn read_u64_from_reader(reader: &mut impl Read) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    Ok(u64::from_be_bytes(bytes))
}

/// Reads one big-endian 32-bit integer from a stream.
fn read_u32_from_reader(reader: &mut impl Read) -> Result<u32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    Ok(u32::from_be_bytes(bytes))
}

/// Reads one big-endian 24-bit integer from a stream.
fn read_u24_from_reader(reader: &mut impl Read) -> Result<u32, String> {
    let mut bytes = [0_u8; 3];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "prototype log file is truncated".to_string())?;
    Ok((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
}

/// Drains one decompression reader into a byte buffer.
fn read_all_from_decoder(mut reader: impl Read) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    reader
        .read_to_end(&mut output)
        .map_err(|error| format!("failed to decompress payload: {error}"))?;
    Ok(output)
}

/// Consumes a known number of bytes from a stream without decoding them.
fn skip_exact(reader: &mut impl Read, length: usize) -> Result<(), String> {
    let mut remaining = length;
    let mut buffer = [0_u8; 1024];
    while remaining > 0 {
        let chunk = usize::min(buffer.len(), remaining);
        reader
            .read_exact(&mut buffer[..chunk])
            .map_err(|_| "prototype log file is truncated".to_string())?;
        remaining -= chunk;
    }
    Ok(())
}

/// Supports deterministic byte parsing without introducing extra dependencies.
#[allow(dead_code)]
struct CursorReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CursorReader<'a> {
    /// Creates a cursor over an immutable slice.
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// Returns the number of unread bytes.
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    /// Reads one big-endian 16-bit integer.
    fn read_u16(&mut self) -> Result<u16, String> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Reads one big-endian 64-bit integer.
    fn read_u64(&mut self) -> Result<u64, String> {
        let bytes = self.read_exact(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Reads one big-endian 32-bit integer.
    fn read_u32(&mut self) -> Result<u32, String> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads one big-endian 24-bit integer.
    fn read_u24(&mut self) -> Result<u32, String> {
        let bytes = self.read_exact(3)?;
        Ok((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
    }

    /// Reads one unsigned byte.
    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    /// Returns a borrowed slice of the requested length.
    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], String> {
        if self.remaining() < length {
            return Err("prototype log file is truncated".to_string());
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }
}

#[cfg(test)]
mod tests {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        format_timestamp_millis, parse_bytes, read_last_window, read_visible_window, scan_file_index, LogLevel,
    };

    fn push_u24(target: &mut Vec<u8>, value: u32) {
        target.push(((value >> 16) & 0xFF) as u8);
        target.push(((value >> 8) & 0xFF) as u8);
        target.push((value & 0xFF) as u8);
    }

    fn build_raw_trunk(lines: &[(u32, u8, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (offset, level, content) in lines {
            bytes.extend_from_slice(&offset.to_be_bytes());
            bytes.push(*level);
            bytes.extend_from_slice(&(content.len() as u32).to_be_bytes());
            bytes.extend_from_slice(content.as_bytes());
        }
        bytes
    }

    fn build_none_file(lines_by_trunk: Vec<Vec<(u32, u8, &str)>>) -> Vec<u8> {
        let base_timestamp = 1_777_672_860_253_u64;
        let total_records = lines_by_trunk
            .iter()
            .map(|trunk| trunk.len() as u64)
            .sum::<u64>();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        bytes.extend_from_slice(&512_u16.to_be_bytes());
        bytes.extend_from_slice(&base_timestamp.to_be_bytes());
        bytes.extend_from_slice(&total_records.to_be_bytes());
        push_u24(&mut bytes, lines_by_trunk.len() as u32);
        for lines in lines_by_trunk {
            let raw_trunk = build_raw_trunk(&lines);
            bytes.extend_from_slice(&(lines.len() as u16).to_be_bytes());
            bytes.extend_from_slice(&(raw_trunk.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&raw_trunk);
        }
        bytes
    }

    fn unique_temp_path(file_name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("tinylog-format-test-{suffix}-{file_name}"))
    }

    #[test]
    fn parse_bytes_reads_two_entries() {
        let bytes = build_none_file(vec![vec![(0, 2, "alpha"), (25, 4, "beta")]]);

        let entries = parse_bytes(&bytes).expect("parse bytes");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].offset_millis, 0);
        assert_eq!(entries[0].level, LogLevel::Info);
        assert_eq!(entries[0].content, "alpha");
        assert_eq!(entries[1].offset_millis, 25);
        assert_eq!(entries[1].level, LogLevel::Error);
        assert_eq!(entries[1].content, "beta");
    }

    #[test]
    fn parse_bytes_rejects_truncated_input() {
        let bytes = vec![0, 1, 2];

        let error = parse_bytes(&bytes).expect_err("truncate error");

        assert!(error.contains("truncated"));
    }

    #[test]
    fn format_timestamp_renders_normal_log_shape() {
        let value = format_timestamp_millis(1_777_672_860_253).expect("format timestamp");

        assert_eq!(value, "2026-05-01 22:01:00,253");
    }

    #[test]
    fn read_visible_window_only_decodes_requested_records() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-visible-window-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut bytes = build_none_file(vec![vec![(0, 2, "alpha")]]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&20_u32.to_be_bytes());
        bytes.extend_from_slice(b"be");
        fs::write(&path, bytes).expect("write prototype file");

        let window =
            read_visible_window(&path.to_string_lossy(), 0, 1).expect("read visible window");

        assert_eq!(window.total_records, 1);
        assert_eq!(window.visible_entries.len(), 1);
        assert_eq!(window.visible_entries[0].content, "alpha");

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn read_last_window_uses_scanned_trunk_index() {
        let path = unique_temp_path("tail.bin");
        let bytes = build_none_file(vec![
            vec![(0, 2, "alpha"), (25, 2, "beta")],
            vec![(50, 3, "gamma"), (75, 4, "delta")],
        ]);
        fs::write(&path, bytes).expect("write prototype file");

        let window = read_last_window(&path.to_string_lossy(), 3).expect("read tail window");

        assert_eq!(window.total_records, 4);
        assert_eq!(window.visible_entries.len(), 3);
        assert_eq!(window.visible_entries[0].content, "beta");
        assert_eq!(window.visible_entries[1].content, "gamma");
        assert_eq!(window.visible_entries[2].content, "delta");

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn read_visible_window_decompresses_gzip_payload() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-visible-gzip-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let raw_trunk = build_raw_trunk(&[(0, 2, "alpha")]);
        let mut gzip_payload = Vec::new();
        {
            let mut encoder = GzEncoder::new(&mut gzip_payload, Compression::default());
            encoder.write_all(&raw_trunk).expect("write gzip payload");
            encoder.finish().expect("finish gzip payload");
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&512_u16.to_be_bytes());
        bytes.extend_from_slice(&1_777_672_860_253_u64.to_be_bytes());
        bytes.extend_from_slice(&1_u64.to_be_bytes());
        push_u24(&mut bytes, 1);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&(gzip_payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&gzip_payload);
        fs::write(&path, bytes).expect("write gzip prototype file");

        let window =
            read_visible_window(&path.to_string_lossy(), 0, 1).expect("read visible window");

        assert_eq!(window.visible_entries[0].content, "alpha");

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn read_visible_window_skips_hidden_trunks_without_decoding() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-visible-skip-{}.tog",
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
        bytes.extend_from_slice(&2_u64.to_be_bytes());
        push_u24(&mut bytes, 2);
        let first_trunk = build_raw_trunk(&[(0, 2, "ba")]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&(first_trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&first_trunk);
        let second_trunk = build_raw_trunk(&[(25, 3, "beta")]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&(second_trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&second_trunk);
        fs::write(&path, bytes).expect("write prototype file");

        let window =
            read_visible_window(&path.to_string_lossy(), 1, 1).expect("read visible window");

        assert_eq!(window.visible_entries.len(), 1);
        assert_eq!(window.visible_entries[0].content, "beta");

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn scan_file_index_tracks_trunk_offsets_and_ranges() {
        let path = unique_temp_path("index.bin");
        let bytes = build_none_file(vec![
            vec![(0, 2, "alpha"), (25, 2, "beta")],
            vec![(50, 3, "gamma")],
        ]);
        fs::write(&path, bytes).expect("write prototype file");

        let index = scan_file_index(&path.to_string_lossy()).expect("scan trunk index");

        assert_eq!(index.total_records(), 3);
        assert_eq!(index.trunk_count(), 2);
        assert_eq!(index.trunk_position_for_record(0), Some(1));
        assert_eq!(index.trunk_position_for_record(2), Some(2));

        fs::remove_file(path).expect("cleanup file");
    }
}
