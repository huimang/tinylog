package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeParseException;
import java.util.List;
import java.util.Objects;

/**
 * Converts a simple plaintext log file into the prototype tinylog binary format.
 */
public final class PlainTextLogToTinylogConverter {
    /**
     * Defines the timestamp pattern used by the source plaintext log file.
     */
    private static final DateTimeFormatter TIMESTAMP_FORMATTER =
            DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss,SSS");

    /**
     * Defines the fixed character width of one leading plaintext timestamp.
     */
    private static final int TIMESTAMP_TEXT_LENGTH = 23;

    /**
     * Stores the compression algorithm used for converted line bodies.
     */
    private final CompressionAlgorithm compressionAlgorithm;

    /**
     * Creates a converter that uses the prototype default compression algorithm.
     */
    public PlainTextLogToTinylogConverter() {
        this(PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM);
    }

    /**
     * Creates a converter that uses the selected line-body compression algorithm.
     */
    public PlainTextLogToTinylogConverter(CompressionAlgorithm compressionAlgorithm) {
        this.compressionAlgorithm = Objects.requireNonNull(compressionAlgorithm, "compressionAlgorithm");
    }

    /**
     * Converts one plaintext log file to one tinylog prototype file.
     *
     * <p>The current prototype accepts lines in the form:
     * {@code <yyyy-MM-dd HH:mm:ss,SSS><space><message>}
     */
    public void convert(Path plainTextLogPath, Path tinylogPath) throws IOException {
        Objects.requireNonNull(plainTextLogPath, "plainTextLogPath");
        Objects.requireNonNull(tinylogPath, "tinylogPath");
        validateTinylogPath(tinylogPath);

        List<String> lines = Files.readAllLines(plainTextLogPath, StandardCharsets.UTF_8);
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(tinylogPath, compressionAlgorithm)) {
            int lineNumber = 0;
            for (String line : lines) {
                lineNumber++;
                if (line.trim().isEmpty()) {
                    continue;
                }
                writer.append(parseLine(plainTextLogPath, lineNumber, line));
            }
        }
    }

    /**
     * Validates that the output file uses the dedicated tinylog extension.
     */
    private void validateTinylogPath(Path tinylogPath) {
        String fileName = tinylogPath.getFileName().toString();
        if (!fileName.endsWith(PrototypeLogFileFormat.FILE_EXTENSION)) {
            throw new IllegalArgumentException("tinylog files must use the "
                    + PrototypeLogFileFormat.FILE_EXTENSION + " extension");
        }
    }

    /**
     * Parses one plaintext line into a prototype log record.
     */
    private LogRecord parseLine(Path plainTextLogPath, int lineNumber, String line) {
        if (line.length() <= TIMESTAMP_TEXT_LENGTH + 1 || line.charAt(TIMESTAMP_TEXT_LENGTH) != ' ') {
            throw new IllegalArgumentException("invalid log line at "
                    + plainTextLogPath + ":" + lineNumber
                    + ", expected '<yyyy-MM-dd HH:mm:ss,SSS> <message>'");
        }
        long timestampMillis;
        try {
            timestampMillis = LocalDateTime.parse(
                    line.substring(0, TIMESTAMP_TEXT_LENGTH),
                    TIMESTAMP_FORMATTER)
                    .atZone(ZoneId.systemDefault())
                    .toInstant()
                    .toEpochMilli();
        } catch (DateTimeParseException exception) {
            throw new IllegalArgumentException("invalid timestamp at "
                    + plainTextLogPath + ":" + lineNumber, exception);
        }
        String message = line.substring(TIMESTAMP_TEXT_LENGTH + 1);
        return new LogRecord(
                timestampMillis,
                LogLevel.INFO,
                plainTextLogPath.getFileName().toString(),
                "converter",
                message,
                null);
    }
}
