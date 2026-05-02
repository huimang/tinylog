use std::{
    fs,
    io::{BufReader, Cursor, Read},
};

use bzip2::read::BzDecoder;
use chrono::{TimeZone, Utc};
use flate2::read::{DeflateDecoder, GzDecoder};
use snap::read::FrameDecoder;
use xz2::read::XzDecoder;
use zip::ZipArchive;

/// Holds the total record count together with the currently visible entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleLogWindow {
    pub total_records: u64,
    pub visible_entries: Vec<ParsedLogEntry>,
}

/// Represents one rendered log entry parsed from the trunk-based tinylog format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLogEntry {
    pub timestamp_millis: u64,
    pub offset_millis: u32,
    pub content: String,
}

/// Holds the parsed file header needed by the viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileHeader {
    compression_algorithm: CompressionAlgorithm,
    base_timestamp_millis: u64,
    total_records: u64,
    trunk_count: u32,
}

/// Enumerates the supported trunk-level compression algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompressionAlgorithm {
    None,
    Gzip,
    Zip,
    Deflate,
    Bzip2,
    Xz,
    Zstd,
    Snappy,
}

/// Reads and parses one trunk-based tinylog file from disk.
#[allow(dead_code)]
pub fn read_file(path: &str) -> Result<Vec<ParsedLogEntry>, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    parse_bytes(&bytes)
}

/// Reads only the currently visible window from one trunk-based tinylog file.
pub fn read_visible_window(
    path: &str,
    start_index: usize,
    visible_count: usize,
) -> Result<VisibleLogWindow, String> {
    let file = fs::File::open(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    let mut reader = BufReader::new(file);
    let header = read_header_from_reader(&mut reader)?;
    let total_records_usize = usize::try_from(header.total_records).unwrap_or(usize::MAX);
    let start_index = usize::min(start_index, total_records_usize);
    let end_index = usize::min(
        start_index.saturating_add(visible_count),
        total_records_usize,
    );
    if start_index >= end_index {
        return Ok(VisibleLogWindow {
            total_records: header.total_records,
            visible_entries: Vec::new(),
        });
    }

    let mut visible_entries = Vec::with_capacity(end_index.saturating_sub(start_index));
    let mut global_index = 0usize;

    for _ in 0..header.trunk_count {
        let trunk_log_line_count = usize::from(read_u16_from_reader(&mut reader)?);
        let compressed_content_length = read_u32_from_reader(&mut reader)? as usize;
        let trunk_start = global_index;
        let trunk_end = global_index.saturating_add(trunk_log_line_count);
        let overlaps_visible_window = trunk_end > start_index && trunk_start < end_index;

        if !overlaps_visible_window {
            skip_exact(&mut reader, compressed_content_length)?;
            global_index = trunk_end;
            continue;
        }

        let mut compressed_content = vec![0_u8; compressed_content_length];
        reader
            .read_exact(&mut compressed_content)
            .map_err(|_| "prototype log file is truncated".to_string())?;
        let raw_trunk_bytes = header
            .compression_algorithm
            .decompress(compressed_content)?;
        let trunk_entries = parse_raw_trunk_payload(
            &raw_trunk_bytes,
            header.base_timestamp_millis,
            trunk_log_line_count,
        )?;
        let local_start = start_index.saturating_sub(trunk_start);
        let local_end = usize::min(trunk_entries.len(), end_index.saturating_sub(trunk_start));
        for entry in trunk_entries
            .into_iter()
            .skip(local_start)
            .take(local_end.saturating_sub(local_start))
        {
            visible_entries.push(entry);
        }
        global_index = trunk_end;
        if visible_entries.len() >= end_index.saturating_sub(start_index) {
            break;
        }
    }

    Ok(VisibleLogWindow {
        total_records: header.total_records,
        visible_entries,
    })
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
        let content_length = cursor.read_u32()? as usize;
        let content_bytes = cursor.read_exact(content_length)?;
        let content = String::from_utf8(content_bytes.to_vec())
            .map_err(|error| format!("invalid utf-8 log content: {error}"))?;
        entries.push(ParsedLogEntry {
            timestamp_millis: base_timestamp_millis + u64::from(offset_millis),
            offset_millis,
            content,
        });
    }
    if cursor.remaining() != 0 {
        return Err("raw trunk payload contains trailing bytes".to_string());
    }
    Ok(entries)
}

impl CompressionAlgorithm {
    /// Resolves one persisted algorithm identifier.
    fn from_id(id: u16) -> Result<Self, String> {
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
}

/// Validates one configured trunk size in KB.
fn validate_trunk_size_kb(trunk_size_kb: u16) -> Result<u16, String> {
    if trunk_size_kb == 0 {
        return Err("trunk size must be greater than zero".to_string());
    }
    Ok(trunk_size_kb)
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

    use super::{format_timestamp_millis, parse_bytes, read_visible_window};

    fn push_u24(target: &mut Vec<u8>, value: u32) {
        target.push(((value >> 16) & 0xFF) as u8);
        target.push(((value >> 8) & 0xFF) as u8);
        target.push((value & 0xFF) as u8);
    }

    fn build_raw_trunk(lines: &[(u32, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (offset, content) in lines {
            bytes.extend_from_slice(&offset.to_be_bytes());
            bytes.extend_from_slice(&(content.len() as u32).to_be_bytes());
            bytes.extend_from_slice(content.as_bytes());
        }
        bytes
    }

    fn build_none_file(lines_by_trunk: Vec<Vec<(u32, &str)>>) -> Vec<u8> {
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

    #[test]
    fn parse_bytes_reads_two_entries() {
        let bytes = build_none_file(vec![vec![(0, "alpha"), (25, "beta")]]);

        let entries = parse_bytes(&bytes).expect("parse bytes");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].offset_millis, 0);
        assert_eq!(entries[0].content, "alpha");
        assert_eq!(entries[1].offset_millis, 25);
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
        let mut bytes = build_none_file(vec![vec![(0, "alpha")]]);
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
    fn read_visible_window_decompresses_gzip_payload() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-visible-gzip-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let raw_trunk = build_raw_trunk(&[(0, "alpha")]);
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
        let first_trunk = build_raw_trunk(&[(0, "ba")]);
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&(first_trunk.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&first_trunk);
        let second_trunk = build_raw_trunk(&[(25, "beta")]);
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
}
