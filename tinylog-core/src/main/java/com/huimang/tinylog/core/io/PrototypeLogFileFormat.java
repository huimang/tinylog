package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.DataInput;
import java.io.DataOutput;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Collections;
import java.util.Properties;

/**
 * Holds the binary layout rules for the current trunk-based tinylog file format.
 */
final class PrototypeLogFileFormat {
    private static final int VERSION_BYTES = 3;
    private static final int BYTES_PER_KB = 1024;
    private static final int MAX_UNSIGNED_MEDIUM = 0xFF_FFFF;
    private static final String VERSION_PROPERTY_NAME = "tinylog.version";

    /**
     * Defines the dedicated tinylog file extension for persisted files.
     */
    static final String FILE_EXTENSION = ".tog";

    /**
     * Defines the default compression algorithm used by the writer and converter.
     */
    static final CompressionAlgorithm DEFAULT_COMPRESSION_ALGORITHM = CompressionAlgorithm.GZIP;

    /**
     * Defines the default trunk size in KB.
     */
    static final int DEFAULT_TRUNK_SIZE_KB = 512;

    /**
     * Stores the maximum trunk size that fits in the unsigned two-byte header field.
     */
    static final int MAX_TRUNK_SIZE_KB = 0xFFFF;

    /**
     * Stores the fixed-size header bytes for version, compression, trunk size, base timestamp, line count,
     * and trunk count.
     */
    static final int HEADER_BYTES = 26;

    /**
     * Stores the fixed-size metadata bytes at the start of one persisted trunk.
     */
    static final int TRUNK_HEADER_BYTES = 6;

    /**
     * Stores the fixed-size metadata bytes for one raw line inside a trunk.
     */
    static final int LINE_HEADER_BYTES = 9;

    /**
     * Stores the byte offset of the base timestamp within the file header.
     */
    static final int BASE_TIMESTAMP_OFFSET = 7;

    /**
     * Stores the byte offset of the total line count within the file header.
     */
    static final int TOTAL_LOG_LINE_COUNT_OFFSET = 15;

    /**
     * Stores the byte offset of the trunk count within the file header.
     */
    static final int TRUNK_COUNT_OFFSET = 23;

    /**
     * Limits one record offset to the 4-byte unsigned millisecond range.
     */
    static final long MAX_OFFSET_MILLIS = 0xFFFF_FFFFL;

    /**
     * Limits one trunk line count to the 2-byte unsigned range.
     */
    static final int MAX_TRUNK_LOG_LINE_COUNT = 0xFFFF;

    /**
     * Defines the UTF-8 encoding used for persisted content bytes.
     */
    static final Charset CONTENT_CHARSET = StandardCharsets.UTF_8;

    /**
     * Caches the three-byte version tuple sourced from the Maven project version.
     */
    private static final byte[] FORMAT_VERSION = loadFormatVersion();

    private PrototypeLogFileFormat() {
    }

    /**
     * Returns the three-byte persisted version tuple.
     */
    static byte[] currentFormatVersion() {
        return Arrays.copyOf(FORMAT_VERSION, FORMAT_VERSION.length);
    }

    /**
     * Writes the current fixed file header.
     */
    static void writeHeader(
            DataOutput output,
            CompressionAlgorithm compressionAlgorithm,
            int trunkSizeKb,
            long baseTimestampUtcMillis,
            long totalLogLineCount,
            int trunkCount) throws IOException {
        output.write(currentFormatVersion());
        output.writeShort(compressionAlgorithm.getId());
        output.writeShort(validateTrunkSizeKb(trunkSizeKb));
        output.writeLong(baseTimestampUtcMillis);
        output.writeLong(totalLogLineCount);
        writeUnsignedMedium(output, trunkCount);
    }

    /**
     * Validates one configured trunk size in KB.
     */
    static int validateTrunkSizeKb(int trunkSizeKb) {
        if (trunkSizeKb <= 0 || trunkSizeKb > MAX_TRUNK_SIZE_KB) {
            throw new IllegalArgumentException("trunk size must fit in the unsigned 2-byte KB header field");
        }
        return trunkSizeKb;
    }

    /**
     * Returns the configured trunk size in bytes.
     */
    static int trunkSizeBytes(int trunkSizeKb) {
        return validateTrunkSizeKb(trunkSizeKb) * BYTES_PER_KB;
    }

    /**
     * Measures the raw bytes required by one line inside a trunk buffer.
     */
    static int measureRawLogLine(LogRecord record) {
        return LINE_HEADER_BYTES + record.getMessage().getBytes(CONTENT_CHARSET).length;
    }

    /**
     * Writes one raw line into the current trunk buffer.
     */
    static void writeRawLogLine(DataOutput output, LogRecord record, long baseTimestampUtcMillis) throws IOException {
        long offsetMillis = record.getTimestampMillis() - baseTimestampUtcMillis;
        if (offsetMillis < 0L || offsetMillis > MAX_OFFSET_MILLIS) {
            throw new IllegalArgumentException("record offset must fit in 4 bytes");
        }
        byte[] contentBytes = record.getMessage().getBytes(CONTENT_CHARSET);
        output.writeInt((int) offsetMillis);
        output.writeByte(toPersistedLevelId(record.getLevel()));
        output.writeInt(contentBytes.length);
        output.write(contentBytes);
    }

    /**
     * Rebuilds a domain record from one parsed raw line.
     */
    static LogRecord toRecord(long baseTimestampUtcMillis, long offsetMillis, int persistedLevelId, String content) {
        return new LogRecord(
                baseTimestampUtcMillis + offsetMillis,
                fromPersistedLevelId(persistedLevelId),
                "prototype",
                "prototype",
                content,
                Collections.singletonMap("format", "trunk"));
    }

    /**
     * Maps one domain level to the persisted one-byte identifier.
     */
    static int toPersistedLevelId(LogLevel level) {
        switch (level) {
            case TRACE:
                return 0;
            case DEBUG:
                return 1;
            case INFO:
                return 2;
            case WARN:
                return 3;
            case ERROR:
                return 4;
            default:
                throw new IllegalArgumentException("unsupported log level: " + level);
        }
    }

    /**
     * Maps one persisted one-byte identifier back to the domain level.
     */
    static LogLevel fromPersistedLevelId(int persistedLevelId) {
        switch (persistedLevelId) {
            case 0:
                return LogLevel.TRACE;
            case 1:
                return LogLevel.DEBUG;
            case 2:
                return LogLevel.INFO;
            case 3:
                return LogLevel.WARN;
            case 4:
                return LogLevel.ERROR;
            default:
                throw new IllegalArgumentException("unsupported persisted log level id: " + persistedLevelId);
        }
    }

    /**
     * Reads one unsigned 24-bit integer in big-endian order.
     */
    static int readUnsignedMedium(DataInput input) throws IOException {
        int first = input.readUnsignedByte();
        int second = input.readUnsignedByte();
        int third = input.readUnsignedByte();
        return (first << 16) | (second << 8) | third;
    }

    /**
     * Writes one unsigned 24-bit integer in big-endian order.
     */
    static void writeUnsignedMedium(DataOutput output, int value) throws IOException {
        if (value < 0 || value > MAX_UNSIGNED_MEDIUM) {
            throw new IllegalArgumentException("value must fit in 3 bytes");
        }
        output.writeByte((value >>> 16) & 0xFF);
        output.writeByte((value >>> 8) & 0xFF);
        output.writeByte(value & 0xFF);
    }

    /**
     * Reads one unsigned 32-bit integer in big-endian order into a Java long.
     */
    static long readUnsignedInt(DataInput input) throws IOException {
        return Integer.toUnsignedLong(input.readInt());
    }

    /**
     * Loads the three-byte version tuple from the Maven-filtered runtime properties.
     */
    private static byte[] loadFormatVersion() {
        Properties properties = new Properties();
        try (InputStream input = PrototypeLogFileFormat.class.getResourceAsStream("/tinylog-core.properties")) {
            if (input == null) {
                throw new IllegalStateException("missing tinylog-core.properties resource");
            }
            properties.load(input);
        } catch (IOException exception) {
            throw new IllegalStateException("failed to load tinylog version metadata", exception);
        }
        String versionText = properties.getProperty(VERSION_PROPERTY_NAME);
        if (versionText == null || versionText.trim().isEmpty()) {
            throw new IllegalStateException("missing tinylog.version property");
        }
        return parseVersionTuple(versionText);
    }

    private static byte[] parseVersionTuple(String versionText) {
        String[] segments = versionText.split("-", 2)[0].split("\\.");
        byte[] version = new byte[] {0, 0, 0};
        for (int index = 0; index < version.length && index < segments.length; index++) {
            version[index] = parseVersionSegment(segments[index]);
        }
        return version;
    }

    private static byte parseVersionSegment(String versionSegment) {
        int value;
        try {
            value = Integer.parseInt(versionSegment);
        } catch (NumberFormatException exception) {
            throw new IllegalStateException("invalid tinylog version segment: " + versionSegment, exception);
        }
        if (value < 0 || value > 0xFF) {
            throw new IllegalStateException("tinylog version segment must fit in one byte");
        }
        return (byte) value;
    }
}
