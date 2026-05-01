package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.DataInput;
import java.io.DataOutput;
import java.io.IOException;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;
import java.util.Collections;

/**
 * Holds the binary layout rules for the current tinylog prototype file format.
 */
final class PrototypeLogFileFormat {
    /**
     * Defines the dedicated tinylog file extension for prototype files.
     */
    static final String FILE_EXTENSION = ".tog";

    /**
     * Defines the default compression algorithm used by the prototype writer and converter.
     */
    static final CompressionAlgorithm DEFAULT_COMPRESSION_ALGORITHM = CompressionAlgorithm.GZIP;

    /**
     * Stores the fixed-size header bytes: compression algorithm plus start timestamp plus record count.
     */
    static final int HEADER_BYTES = 17;

    /**
     * Stores the fixed-size metadata bytes for one record: offset plus content length.
     */
    static final int ENTRY_METADATA_BYTES = 7;

    /**
     * Limits one record offset to the 4-byte unsigned millisecond range.
     */
    static final long MAX_OFFSET_MILLIS = 0xFFFF_FFFFL;

    /**
     * Limits one record payload to the 3-byte unsigned length range.
     */
    static final int MAX_CONTENT_LENGTH = 0xFF_FFFF;

    /**
     * Defines the UTF-8 encoding used for the prototype content bytes.
     */
    static final Charset CONTENT_CHARSET = StandardCharsets.UTF_8;

    private PrototypeLogFileFormat() {
    }

    /**
     * Converts the current record payload to the prototype UTF-8 content field.
     */
    static byte[] toContentBytes(LogRecord record, CompressionAlgorithm compressionAlgorithm) throws IOException {
        return compressionAlgorithm.compress(record.getMessage().getBytes(CONTENT_CHARSET));
    }

    /**
     * Rebuilds a domain record from the current prototype payload.
     */
    static LogRecord toRecord(long startTimestampMillis, long offsetMillis, String content) {
        return new LogRecord(
                startTimestampMillis + offsetMillis,
                LogLevel.INFO,
                "prototype",
                "prototype",
                content,
                Collections.singletonMap("format", "prototype"));
    }

    /**
     * Writes one unsigned 24-bit integer in big-endian order.
     */
    static void writeUnsignedMedium(DataOutput output, int value) throws IOException {
        if (value < 0 || value > MAX_CONTENT_LENGTH) {
            throw new IllegalArgumentException("content length must fit in 3 bytes");
        }
        output.writeByte((value >>> 16) & 0xFF);
        output.writeByte((value >>> 8) & 0xFF);
        output.writeByte(value & 0xFF);
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
     * Reads one unsigned 32-bit integer in big-endian order into a Java long.
     */
    static long readUnsignedInt(DataInput input) throws IOException {
        return Integer.toUnsignedLong(input.readInt());
    }
}
