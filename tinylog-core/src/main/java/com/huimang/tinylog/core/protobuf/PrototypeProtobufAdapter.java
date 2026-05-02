package com.huimang.tinylog.core.protobuf;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import com.google.protobuf.Int32Value;
import com.google.protobuf.StringValue;
import com.google.protobuf.UInt64Value;
import com.huimang.tinylog.proto.PrototypeLogLevel;
import com.huimang.tinylog.proto.PrototypeLogQuery;
import com.huimang.tinylog.proto.PrototypeLogRecord;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/**
 * Maps TinyLog core domain objects to and from the shared protobuf contract.
 */
public final class PrototypeProtobufAdapter {
    private PrototypeProtobufAdapter() {
    }

    /**
     * Converts one domain log record to the shared protobuf contract.
     */
    public static PrototypeLogRecord toProtoRecord(LogRecord record) {
        Objects.requireNonNull(record, "record");
        return PrototypeLogRecord.newBuilder()
                .setTimestampMillis(record.getTimestampMillis())
                .setLevel(toProtoLevel(record.getLevel()))
                .setSource(record.getSource())
                .setContext(record.getContext())
                .setMessage(record.getMessage())
                .putAllAttributes(record.getAttributes())
                .build();
    }

    /**
     * Converts one shared protobuf record to the TinyLog domain model.
     */
    public static LogRecord toDomainRecord(PrototypeLogRecord record) {
        Objects.requireNonNull(record, "record");
        Map<String, String> attributes = new LinkedHashMap<String, String>(record.getAttributesMap());
        return new LogRecord(
                record.getTimestampMillis(),
                toDomainLevel(record.getLevel()),
                record.getSource(),
                record.getContext(),
                record.getMessage(),
                attributes);
    }

    /**
     * Converts one domain query to the shared protobuf contract.
     */
    public static PrototypeLogQuery toProtoQuery(LogQuery query) {
        Objects.requireNonNull(query, "query");
        PrototypeLogQuery.Builder builder = PrototypeLogQuery.newBuilder();
        if (query.getStartTimestampMillis() != null) {
            builder.setStartTimestampMillis(UInt64Value.of(query.getStartTimestampMillis().longValue()));
        }
        if (query.getEndTimestampMillis() != null) {
            builder.setEndTimestampMillis(UInt64Value.of(query.getEndTimestampMillis().longValue()));
        }
        if (query.getMinimumLevel() != null) {
            builder.setMinimumLevel(Int32Value.of(toProtoLevel(query.getMinimumLevel()).getNumber()));
        }
        if (query.getKeyword() != null) {
            builder.setKeyword(StringValue.of(query.getKeyword()));
        }
        return builder.build();
    }

    /**
     * Converts one shared protobuf query to the TinyLog domain model.
     */
    public static LogQuery toDomainQuery(PrototypeLogQuery query) {
        Objects.requireNonNull(query, "query");
        LogQuery.Builder builder = LogQuery.builder();
        if (query.hasStartTimestampMillis()) {
            builder.startTimestampMillis(Long.valueOf(query.getStartTimestampMillis().getValue()));
        }
        if (query.hasEndTimestampMillis()) {
            builder.endTimestampMillis(Long.valueOf(query.getEndTimestampMillis().getValue()));
        }
        if (query.hasMinimumLevel()) {
            builder.minimumLevel(toDomainLevel(PrototypeLogLevel.forNumber(query.getMinimumLevel().getValue())));
        }
        if (query.hasKeyword()) {
            builder.keyword(query.getKeyword().getValue());
        }
        return builder.build();
    }

    private static PrototypeLogLevel toProtoLevel(LogLevel level) {
        switch (level) {
            case TRACE:
                return PrototypeLogLevel.PROTOTYPE_LOG_LEVEL_TRACE;
            case DEBUG:
                return PrototypeLogLevel.PROTOTYPE_LOG_LEVEL_DEBUG;
            case INFO:
                return PrototypeLogLevel.PROTOTYPE_LOG_LEVEL_INFO;
            case WARN:
                return PrototypeLogLevel.PROTOTYPE_LOG_LEVEL_WARN;
            case ERROR:
                return PrototypeLogLevel.PROTOTYPE_LOG_LEVEL_ERROR;
            default:
                throw new IllegalArgumentException("unsupported log level: " + level);
        }
    }

    private static LogLevel toDomainLevel(PrototypeLogLevel level) {
        if (level == null) {
            throw new IllegalArgumentException("unsupported protobuf log level: null");
        }
        switch (level) {
            case PROTOTYPE_LOG_LEVEL_TRACE:
                return LogLevel.TRACE;
            case PROTOTYPE_LOG_LEVEL_DEBUG:
                return LogLevel.DEBUG;
            case PROTOTYPE_LOG_LEVEL_INFO:
                return LogLevel.INFO;
            case PROTOTYPE_LOG_LEVEL_WARN:
                return LogLevel.WARN;
            case PROTOTYPE_LOG_LEVEL_ERROR:
                return LogLevel.ERROR;
            case UNRECOGNIZED:
            case PROTOTYPE_LOG_LEVEL_UNSPECIFIED:
            default:
                throw new IllegalArgumentException("unsupported protobuf log level: " + level);
        }
    }
}
