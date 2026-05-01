use bzip2::read::BzDecoder;
use chrono::{Local, TimeZone};
use flate2::read::{DeflateDecoder, GzDecoder};
use snap::read::FrameDecoder;
use std::fs;
use std::io::{BufReader, Cursor, Read};
use xz2::read::XzDecoder;
use zip::ZipArchive;

/// Holds the total record count together with the currently visible entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleLogWindow {
    pub total_records: u64,
    pub visible_entries: Vec<ParsedLogEntry>,
}

/// Represents one rendered log entry parsed from the prototype binary format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLogEntry {
    pub timestamp_millis: u64,
    pub offset_millis: u32,
    pub content: String,
}

/// Enumerates the supported header-level message compression algorithms.
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

/// Reads and parses one prototype tinylog file from disk.
#[allow(dead_code)]
pub fn read_file(path: &str) -> Result<Vec<ParsedLogEntry>, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    parse_bytes(&bytes)
}

/// Reads only the currently visible window from one prototype tinylog file.
pub fn read_visible_window(path: &str, start_index: usize, visible_count: usize) -> Result<VisibleLogWindow, String> {
    let file = fs::File::open(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    let mut reader = BufReader::new(file);
    let compression_algorithm = CompressionAlgorithm::from_id(read_u16_from_reader(&mut reader)?)?;
    let start_timestamp_millis = read_u64_from_reader(&mut reader)?;
    let total_records = read_u64_from_reader(&mut reader)?;
    let total_records_usize = usize::try_from(total_records).unwrap_or(usize::MAX);
    let start_index = usize::min(start_index, total_records_usize);
    let visible_count = usize::min(
        visible_count,
        total_records_usize.saturating_sub(start_index),
    );
    let mut visible_entries = Vec::with_capacity(visible_count);

    for _ in 0..start_index {
        let _offset_millis = read_u32_from_reader(&mut reader)?;
        let content_length = read_u24_from_reader(&mut reader)? as usize;
        skip_exact(&mut reader, content_length)?;
    }

    for _ in 0..visible_count {
        let offset_millis = read_u32_from_reader(&mut reader)?;
        let content_length = read_u24_from_reader(&mut reader)? as usize;
        let mut content_bytes = vec![0_u8; content_length];
        reader
            .read_exact(&mut content_bytes)
            .map_err(|_| "prototype log file is truncated".to_string())?;
        let content = String::from_utf8(compression_algorithm.decompress(content_bytes)?)
            .map_err(|error| format!("invalid utf-8 log content: {error}"))?;
        visible_entries.push(ParsedLogEntry {
            timestamp_millis: start_timestamp_millis + u64::from(offset_millis),
            offset_millis,
            content,
        });
    }

    Ok(VisibleLogWindow {
        total_records,
        visible_entries,
    })
}

/// Formats epoch milliseconds as a human-readable local timestamp.
pub fn format_timestamp_millis(timestamp_millis: u64) -> Result<String, String> {
    let timestamp_millis =
        i64::try_from(timestamp_millis).map_err(|_| "timestamp exceeds supported range".to_string())?;
    let date_time = Local
        .timestamp_millis_opt(timestamp_millis)
        .single()
        .ok_or_else(|| "timestamp cannot be represented in the local time zone".to_string())?;
    Ok(date_time.format("%Y-%m-%d %H:%M:%S,%3f").to_string())
}

/// Parses bytes using the current prototype layout.
#[allow(dead_code)]
pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<ParsedLogEntry>, String> {
    let mut cursor = CursorReader::new(bytes);
    let compression_algorithm = CompressionAlgorithm::from_id(cursor.read_u16()?)?;
    let start_timestamp_millis = cursor.read_u64()?;
    let record_count = cursor.read_u64()?;
    let mut entries = Vec::new();

    for _ in 0..record_count {
        let offset_millis = cursor.read_u32()?;
        let content_length = cursor.read_u24()? as usize;
        let content_bytes = cursor.read_exact(content_length)?;
        let content = String::from_utf8(compression_algorithm.decompress(content_bytes.to_vec())?)
            .map_err(|error| format!("invalid utf-8 log content: {error}"))?;
        entries.push(ParsedLogEntry {
            timestamp_millis: start_timestamp_millis + u64::from(offset_millis),
            offset_millis,
            content,
        });
    }

    if cursor.remaining() != 0 {
        return Err("unexpected trailing bytes after parsing prototype file".to_string());
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

    /// Decompresses one record payload according to the selected header algorithm.
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
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{format_timestamp_millis, parse_bytes, read_visible_window};

    /**
     * Builds one valid two-record prototype buffer for parser tests.
     */
    fn sample_bytes() -> Vec<u8> {
        vec![
            0, 0,
            0, 0, 1, 139, 207, 229, 104, 0,
            0, 0, 0, 0, 0, 0, 0, 2,
            0, 0, 0, 0, 0, 0, 5, b'a', b'l', b'p', b'h', b'a',
            0, 0, 0, 25, 0, 0, 4, b'b', b'e', b't', b'a',
        ]
    }

    #[test]
    fn parse_bytes_reads_two_entries() {
        let bytes = sample_bytes();

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
        let value = format_timestamp_millis(1_777_658_460_253).expect("format timestamp");

        assert_eq!(value.len(), 23);
        assert_eq!(&value[4..5], "-");
        assert_eq!(&value[7..8], "-");
        assert_eq!(&value[10..11], " ");
        assert_eq!(&value[13..14], ":");
        assert_eq!(&value[16..17], ":");
        assert_eq!(&value[19..20], ",");
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
        fs::write(
            &path,
            vec![
                0, 0,
                0, 0, 1, 139, 207, 229, 104, 0,
                0, 0, 0, 0, 0, 0, 0, 2,
                0, 0, 0, 0, 0, 0, 5, b'a', b'l', b'p', b'h', b'a',
                0, 0, 0, 25, 0, 0, 20, b'b', b'e',
            ],
        )
        .expect("write prototype file");

        let window = read_visible_window(&path.to_string_lossy(), 0, 1).expect("read visible window");

        assert_eq!(window.total_records, 2);
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
        let mut gzip_payload = Vec::new();
        {
            let mut encoder = GzEncoder::new(&mut gzip_payload, Compression::default());
            encoder.write_all(b"alpha").expect("write gzip payload");
            encoder.finish().expect("finish gzip payload");
        }
        let mut bytes = vec![0, 1];
        bytes.extend_from_slice(&[0, 0, 1, 139, 207, 229, 104, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.push(((gzip_payload.len() >> 16) & 0xFF) as u8);
        bytes.push(((gzip_payload.len() >> 8) & 0xFF) as u8);
        bytes.push((gzip_payload.len() & 0xFF) as u8);
        bytes.extend_from_slice(&gzip_payload);
        fs::write(&path, bytes).expect("write gzip prototype file");

        let window = read_visible_window(&path.to_string_lossy(), 0, 1).expect("read visible window");

        assert_eq!(window.visible_entries[0].content, "alpha");

        fs::remove_file(path).expect("cleanup file");
    }

    #[test]
    fn read_visible_window_skips_hidden_records_without_decoding() {
        let path = std::env::temp_dir().join(format!(
            "tinylog-visible-skip-{}.tog",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            vec![
                0, 0,
                0, 0, 1, 139, 207, 229, 104, 0,
                0, 0, 0, 0, 0, 0, 0, 2,
                0, 0, 0, 0, 0, 0, 2, b'b', b'a',
                0, 0, 0, 25, 0, 0, 4, b'b', b'e', b't', b'a',
            ],
        )
        .expect("write prototype file");

        let window = read_visible_window(&path.to_string_lossy(), 1, 1).expect("read visible window");

        assert_eq!(window.visible_entries.len(), 1);
        assert_eq!(window.visible_entries[0].content, "beta");

        fs::remove_file(path).expect("cleanup file");
    }
}
