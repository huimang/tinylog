use chrono::{Local, TimeZone};
use std::fs;

/// Represents one rendered log entry parsed from the prototype binary format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLogEntry {
    pub timestamp_millis: u64,
    pub offset_millis: u32,
    pub content: String,
}

/// Reads and parses one prototype tinylog file from disk.
pub fn read_file(path: &str) -> Result<Vec<ParsedLogEntry>, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    parse_bytes(&bytes)
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
pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<ParsedLogEntry>, String> {
    let mut cursor = Cursor::new(bytes);
    let start_timestamp_millis = cursor.read_u64()?;
    let record_count = cursor.read_u64()?;
    let mut entries = Vec::new();

    for _ in 0..record_count {
        let offset_millis = cursor.read_u32()?;
        let content_length = cursor.read_u24()? as usize;
        let content_bytes = cursor.read_exact(content_length)?;
        let content = String::from_utf8(content_bytes.to_vec())
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

/// Supports deterministic byte parsing without introducing extra dependencies.
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    /// Creates a cursor over an immutable slice.
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// Returns the number of unread bytes.
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
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
    use super::{format_timestamp_millis, parse_bytes};

    /**
     * Builds one valid two-record prototype buffer for parser tests.
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
}
