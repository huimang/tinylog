package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.BufferedReader;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeParseException;
import java.util.Locale;
import java.util.Objects;

/**
 * Converts a plaintext log file into the current trunk-based tinylog format.
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
    private static final char TIMESTAMP_SEPARATOR = ' ';
    private static final String CONVERTER_SOURCE = "converter";

    /**
     * Stores the compression algorithm used for completed trunks.
     */
    private final CompressionAlgorithm compressionAlgorithm;

    /**
     * Stores the configured trunk size in KB.
     */
    private final int trunkSizeKb;

    /**
     * Creates a converter that uses the default compression and trunk size.
     */
    public PlainTextLogToTinylogConverter() {
        this(PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM, PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB);
    }

    /**
     * Creates a converter that uses the selected compression and default trunk size.
     */
    public PlainTextLogToTinylogConverter(CompressionAlgorithm compressionAlgorithm) {
        this(compressionAlgorithm, PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB);
    }

    /**
     * Creates a converter that uses the selected compression and trunk size.
     */
    public PlainTextLogToTinylogConverter(CompressionAlgorithm compressionAlgorithm, int trunkSizeKb) {
        this.compressionAlgorithm = Objects.requireNonNull(compressionAlgorithm, "compressionAlgorithm");
        this.trunkSizeKb = PrototypeLogFileFormat.validateTrunkSizeKb(trunkSizeKb);
    }

    /**
     * Converts one plaintext log file to one trunk-based tinylog file.
     *
     * <p>The accepted line format is {@code <yyyy-MM-dd HH:mm:ss,SSS><space>[LEVEL]<space><message>}. The converter
     * removes that first level token from the stored message and persists the level in the raw tinylog line metadata.
     * The timestamp text is interpreted as a UTC calendar value to match the file-level UTC base timestamp.
     */
    public void convert(Path plainTextLogPath, Path tinylogPath) throws IOException {
        Objects.requireNonNull(plainTextLogPath, "plainTextLogPath");
        Objects.requireNonNull(tinylogPath, "tinylogPath");
        validateTinylogPath(tinylogPath);

        try (BufferedReader reader = Files.newBufferedReader(plainTextLogPath, PrototypeLogFileFormat.CONTENT_CHARSET);
                PrototypeLogFileWriter writer =
                        new PrototypeLogFileWriter(tinylogPath, compressionAlgorithm, trunkSizeKb)) {
            int lineNumber = 0;
            String line;
            while ((line = reader.readLine()) != null) {
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
     * Parses one plaintext line into one logical log record.
     */
    private LogRecord parseLine(Path plainTextLogPath, int lineNumber, String line) {
        validateLineShape(plainTextLogPath, lineNumber, line);
        long timestampMillis = parseTimestampMillis(plainTextLogPath, lineNumber, line);
        ParsedPlainTextLine parsedLine = parseLevelAndMessage(line);
        return new LogRecord(
                timestampMillis,
                parsedLine.level,
                plainTextLogPath.getFileName().toString(),
                CONVERTER_SOURCE,
                parsedLine.message,
                null);
    }

    private void validateLineShape(Path plainTextLogPath, int lineNumber, String line) {
        if (line.length() <= TIMESTAMP_TEXT_LENGTH + 1 || line.charAt(TIMESTAMP_TEXT_LENGTH) != TIMESTAMP_SEPARATOR) {
            throw new IllegalArgumentException("invalid log line at "
                    + plainTextLogPath + ":" + lineNumber
                    + ", expected '<yyyy-MM-dd HH:mm:ss,SSS> <message>'");
        }
    }

    private long parseTimestampMillis(Path plainTextLogPath, int lineNumber, String line) {
        try {
            return LocalDateTime.parse(line.substring(0, TIMESTAMP_TEXT_LENGTH), TIMESTAMP_FORMATTER)
                    .toInstant(ZoneOffset.UTC)
                    .toEpochMilli();
        } catch (DateTimeParseException exception) {
            throw new IllegalArgumentException(
                    "invalid timestamp at " + plainTextLogPath + ":" + lineNumber,
                    exception);
        }
    }

    private ParsedPlainTextLine parseLevelAndMessage(String line) {
        String content = line.substring(TIMESTAMP_TEXT_LENGTH + 1);
        if (!content.startsWith("[")) {
            return new ParsedPlainTextLine(LogLevel.INFO, content);
        }
        int closingBracketIndex = content.indexOf(']');
        if (closingBracketIndex < 0) {
            return new ParsedPlainTextLine(LogLevel.INFO, content);
        }
        LogLevel level = tryParseLevelToken(content.substring(1, closingBracketIndex));
        if (level == null) {
            return new ParsedPlainTextLine(LogLevel.INFO, content);
        }
        String message = content.substring(closingBracketIndex + 1);
        if (!message.isEmpty() && message.charAt(0) == TIMESTAMP_SEPARATOR) {
            message = message.substring(1);
        }
        return new ParsedPlainTextLine(level, message);
    }

    private LogLevel tryParseLevelToken(String levelToken) {
        String normalizedLevel = levelToken.trim().toUpperCase(Locale.ROOT);
        if ("FATAL".equals(normalizedLevel)) {
            return LogLevel.ERROR;
        }
        for (LogLevel value : LogLevel.values()) {
            if (value.name().equals(normalizedLevel)) {
                return value;
            }
        }
        return null;
    }

    private static final class ParsedPlainTextLine {
        private final LogLevel level;
        private final String message;

        private ParsedPlainTextLine(LogLevel level, String message) {
            this.level = level;
            this.message = message;
        }
    }
}
