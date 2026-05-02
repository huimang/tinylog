use std::collections::BTreeMap;

use crate::format;

/// Generated protobuf messages for the shared TinyLog prototype contract.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/tinylog.prototype.v1.rs"));
}

/// Represents the shared severity contract on the Rust side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtobufLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Represents one Rust-side log record mapped to the shared protobuf contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtobufLogRecord {
    pub timestamp_millis: u64,
    pub level: ProtobufLogLevel,
    pub source: String,
    pub context: String,
    pub message: String,
    pub attributes: BTreeMap<String, String>,
}

/// Represents one Rust-side log query mapped to the shared protobuf contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtobufLogQuery {
    pub start_timestamp_millis: Option<u64>,
    pub end_timestamp_millis: Option<u64>,
    pub minimum_level: Option<ProtobufLogLevel>,
    pub keyword: Option<String>,
}

impl From<format::LogLevel> for ProtobufLogLevel {
    fn from(level: format::LogLevel) -> Self {
        match level {
            format::LogLevel::Trace => Self::Trace,
            format::LogLevel::Debug => Self::Debug,
            format::LogLevel::Info => Self::Info,
            format::LogLevel::Warn => Self::Warn,
            format::LogLevel::Error => Self::Error,
        }
    }
}

impl From<ProtobufLogLevel> for format::LogLevel {
    fn from(level: ProtobufLogLevel) -> Self {
        match level {
            ProtobufLogLevel::Trace => Self::Trace,
            ProtobufLogLevel::Debug => Self::Debug,
            ProtobufLogLevel::Info => Self::Info,
            ProtobufLogLevel::Warn => Self::Warn,
            ProtobufLogLevel::Error => Self::Error,
        }
    }
}

impl From<ProtobufLogLevel> for i32 {
    fn from(level: ProtobufLogLevel) -> Self {
        match level {
            ProtobufLogLevel::Trace => proto::PrototypeLogLevel::Trace as i32,
            ProtobufLogLevel::Debug => proto::PrototypeLogLevel::Debug as i32,
            ProtobufLogLevel::Info => proto::PrototypeLogLevel::Info as i32,
            ProtobufLogLevel::Warn => proto::PrototypeLogLevel::Warn as i32,
            ProtobufLogLevel::Error => proto::PrototypeLogLevel::Error as i32,
        }
    }
}

impl TryFrom<i32> for ProtobufLogLevel {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, String> {
        match proto::PrototypeLogLevel::try_from(value) {
            Ok(proto::PrototypeLogLevel::Trace) => Ok(Self::Trace),
            Ok(proto::PrototypeLogLevel::Debug) => Ok(Self::Debug),
            Ok(proto::PrototypeLogLevel::Info) => Ok(Self::Info),
            Ok(proto::PrototypeLogLevel::Warn) => Ok(Self::Warn),
            Ok(proto::PrototypeLogLevel::Error) => Ok(Self::Error),
            Ok(proto::PrototypeLogLevel::Unspecified) => {
                Err("protobuf log level is unspecified".to_string())
            }
            Err(_) => Err(format!("unsupported protobuf log level: {value}")),
        }
    }
}

impl From<ProtobufLogRecord> for proto::PrototypeLogRecord {
    fn from(record: ProtobufLogRecord) -> Self {
        Self {
            timestamp_millis: record.timestamp_millis,
            level: i32::from(record.level),
            source: record.source,
            context: record.context,
            message: record.message,
            attributes: record.attributes.into_iter().collect(),
        }
    }
}

impl TryFrom<proto::PrototypeLogRecord> for ProtobufLogRecord {
    type Error = String;

    fn try_from(record: proto::PrototypeLogRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            timestamp_millis: record.timestamp_millis,
            level: ProtobufLogLevel::try_from(record.level)?,
            source: record.source,
            context: record.context,
            message: record.message,
            attributes: record.attributes.into_iter().collect(),
        })
    }
}

impl From<ProtobufLogQuery> for proto::PrototypeLogQuery {
    fn from(query: ProtobufLogQuery) -> Self {
        Self {
            start_timestamp_millis: query.start_timestamp_millis,
            end_timestamp_millis: query.end_timestamp_millis,
            minimum_level: query.minimum_level.map(i32::from),
            keyword: query.keyword,
        }
    }
}

impl TryFrom<proto::PrototypeLogQuery> for ProtobufLogQuery {
    type Error = String;

    fn try_from(query: proto::PrototypeLogQuery) -> Result<Self, Self::Error> {
        Ok(Self {
            start_timestamp_millis: query.start_timestamp_millis,
            end_timestamp_millis: query.end_timestamp_millis,
            minimum_level: query.minimum_level.map(ProtobufLogLevel::try_from).transpose()?,
            keyword: query.keyword,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{proto, ProtobufLogLevel, ProtobufLogQuery, ProtobufLogRecord};
    use std::collections::BTreeMap;

    #[test]
    fn should_round_trip_protobuf_log_record() {
        let mut attributes = BTreeMap::new();
        attributes.insert("tenant".to_string(), "blue".to_string());
        let record = ProtobufLogRecord {
            timestamp_millis: 1_777_672_860_253,
            level: ProtobufLogLevel::Warn,
            source: "order-service".to_string(),
            context: "queue-monitor".to_string(),
            message: "queue depth is rising".to_string(),
            attributes,
        };

        let encoded: proto::PrototypeLogRecord = record.clone().into();
        let decoded = ProtobufLogRecord::try_from(encoded).expect("record should decode");

        assert_eq!(record, decoded);
    }

    #[test]
    fn should_round_trip_protobuf_log_query() {
        let query = ProtobufLogQuery {
            start_timestamp_millis: Some(100),
            end_timestamp_millis: Some(200),
            minimum_level: Some(ProtobufLogLevel::Error),
            keyword: Some("payment".to_string()),
        };

        let encoded: proto::PrototypeLogQuery = query.clone().into();
        let decoded = ProtobufLogQuery::try_from(encoded).expect("query should decode");

        assert_eq!(query, decoded);
    }
}
