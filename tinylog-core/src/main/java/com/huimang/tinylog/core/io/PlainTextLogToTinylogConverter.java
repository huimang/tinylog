package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Objects;

/**
 * Converts a simple plaintext log file into the prototype tinylog binary format.
 */
public final class PlainTextLogToTinylogConverter {
    /**
     * Converts one plaintext log file to one tinylog prototype file.
     *
     * <p>The current prototype accepts lines in the form:
     * {@code <epochMillis><space><message>}
     */
    public void convert(Path plainTextLogPath, Path tinylogPath) throws IOException {
        Objects.requireNonNull(plainTextLogPath, "plainTextLogPath");
        Objects.requireNonNull(tinylogPath, "tinylogPath");
        validateTinylogPath(tinylogPath);

        List<String> lines = Files.readAllLines(plainTextLogPath, StandardCharsets.UTF_8);
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(tinylogPath)) {
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
        int separatorIndex = line.indexOf(' ');
        if (separatorIndex <= 0 || separatorIndex == line.length() - 1) {
            throw new IllegalArgumentException("invalid log line at "
                    + plainTextLogPath + ":" + lineNumber
                    + ", expected '<epochMillis> <message>'");
        }
        long timestampMillis;
        try {
            timestampMillis = Long.parseLong(line.substring(0, separatorIndex));
        } catch (NumberFormatException exception) {
            throw new IllegalArgumentException("invalid timestamp at "
                    + plainTextLogPath + ":" + lineNumber, exception);
        }
        String message = line.substring(separatorIndex + 1);
        return new LogRecord(
                timestampMillis,
                LogLevel.INFO,
                plainTextLogPath.getFileName().toString(),
                "converter",
                message,
                null);
    }
}
