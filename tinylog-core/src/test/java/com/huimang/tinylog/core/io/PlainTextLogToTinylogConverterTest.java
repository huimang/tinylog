package com.huimang.tinylog.core.io;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class PlainTextLogToTinylogConverterTest {
    @TempDir
    Path tempDir;

    @Test
    void shouldConvertNormalLogToTinylogPrototypeFile() throws IOException {
        Path normalLogPath = tempDir.resolve("normal.log");
        Path tinylogPath = tempDir.resolve("normal.tog");
        Files.write(
                normalLogPath,
                java.util.Arrays.asList(
                        "2026-05-01 22:01:00,253 service started",
                        "2026-05-01 22:01:00,278 user signed in"),
                StandardCharsets.UTF_8);

        new PlainTextLogToTinylogConverter().convert(normalLogPath, tinylogPath);

        List<LogRecord> records;
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(tinylogPath)) {
            records = collect(reader.scan());
        }

        assertEquals(2, records.size());
        assertEquals(toEpochMillis("2026-05-01 22:01:00,253"), records.get(0).getTimestampMillis());
        assertEquals(toEpochMillis("2026-05-01 22:01:00,278"), records.get(1).getTimestampMillis());
        assertEquals("service started", records.get(0).getMessage());
        assertEquals("user signed in", records.get(1).getMessage());
    }

    @Test
    void shouldRejectNonTogOutputFiles() throws IOException {
        Path normalLogPath = tempDir.resolve("normal.log");
        Files.write(
                normalLogPath,
                Collections.singletonList("2026-05-01 22:01:00,253 service started"),
                StandardCharsets.UTF_8);

        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> new PlainTextLogToTinylogConverter().convert(normalLogPath, tempDir.resolve("normal.log.bin")));

        assertEquals("tinylog files must use the .tog extension", error.getMessage());
    }

    @Test
    void shouldRejectInvalidPlainTextTimestamp() throws IOException {
        Path normalLogPath = tempDir.resolve("normal.log");
        Path tinylogPath = tempDir.resolve("normal.tog");
        Files.write(
                normalLogPath,
                Collections.singletonList("2026-05-01 22:01:xx,253 service started"),
                StandardCharsets.UTF_8);

        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> new PlainTextLogToTinylogConverter().convert(normalLogPath, tinylogPath));

        assertEquals("invalid timestamp at " + normalLogPath + ":1", error.getMessage());
    }

    /**
     * Drains one iterator into a stable list for assertions.
     */
    private static List<LogRecord> collect(Iterator<LogRecord> iterator) {
        List<LogRecord> result = new ArrayList<LogRecord>();
        while (iterator.hasNext()) {
            result.add(iterator.next());
        }
        return Collections.unmodifiableList(result);
    }

    /**
     * Converts one local timestamp string to epoch milliseconds using the host zone.
     */
    private static long toEpochMillis(String value) {
        return LocalDateTime.parse(value, DateTimeFormatterHolder.TIMESTAMP_FORMATTER)
                .atZone(ZoneId.systemDefault())
                .toInstant()
                .toEpochMilli();
    }

    /**
     * Holds shared test parsing rules without duplicating formatter literals.
     */
    private static final class DateTimeFormatterHolder {
        /**
         * Matches the plaintext timestamp format accepted by the converter.
         */
        private static final java.time.format.DateTimeFormatter TIMESTAMP_FORMATTER =
                java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss,SSS");

        private DateTimeFormatterHolder() {
        }
    }
}
