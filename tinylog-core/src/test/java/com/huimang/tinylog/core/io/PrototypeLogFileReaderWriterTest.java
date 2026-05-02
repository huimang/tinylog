package com.huimang.tinylog.core.io;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class PrototypeLogFileReaderWriterTest {
    @TempDir
    Path tempDir;

    @Test
    void shouldRoundTripPrototypeFile() throws IOException {
        Path path = tempDir.resolve("sample.tog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path, CompressionAlgorithm.GZIP, 1)) {
            writer.append(new LogRecord(1_777_672_860_253L, LogLevel.INFO, "app", "main", "alpha", null));
            writer.append(new LogRecord(1_777_672_860_378L, LogLevel.ERROR, "app", "main", "beta", null));
        }

        List<LogRecord> records;
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(path)) {
            records = collect(reader.scan());
        }

        assertEquals(2, records.size());
        assertEquals(1_777_672_860_253L, records.get(0).getTimestampMillis());
        assertEquals(1_777_672_860_378L, records.get(1).getTimestampMillis());
        assertEquals("alpha", records.get(0).getMessage());
        assertEquals("beta", records.get(1).getMessage());
        assertEquals("trunk", records.get(0).getAttributes().get("format"));
    }

    @Test
    void shouldPersistHeaderVersionAndCountersAcrossMultipleTrunks() throws IOException {
        Path path = tempDir.resolve("multi-trunk.tog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path, CompressionAlgorithm.GZIP, 1)) {
            writer.append(new LogRecord(1_777_672_860_253L, LogLevel.INFO, "app", "main", repeat('a', 700), null));
            writer.append(new LogRecord(1_777_672_860_278L, LogLevel.INFO, "app", "main", repeat('b', 700), null));
            writer.append(new LogRecord(1_777_672_860_353L, LogLevel.INFO, "app", "main", repeat('c', 700), null));
        }

        byte[] bytes = Files.readAllBytes(path);

        assertArrayEquals(new byte[] {0, 1, 0}, readVersion(bytes));
        assertEquals(CompressionAlgorithm.GZIP.getId(), readCompressionAlgorithm(bytes));
        assertEquals(1, readTrunkSizeKb(bytes));
        assertEquals(1_777_672_860_253L, readBaseTimestamp(bytes));
        assertEquals(3L, readTotalLogLineCount(bytes));
        assertEquals(3, readTrunkCount(bytes));
    }

    @Test
    void shouldDeleteTemporaryBufferFilesAfterClose() throws IOException {
        Path path = tempDir.resolve("cleanup.tog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path, CompressionAlgorithm.GZIP, 1)) {
            writer.append(new LogRecord(1_777_672_860_253L, LogLevel.INFO, "app", "main", "alpha", null));
        }

        assertFalse(Files.exists(tempDir.resolve("log-buffer-0.tmp")));
        assertFalse(Files.exists(tempDir.resolve("log-buffer-1.tmp")));
    }

    @Test
    void shouldFilterPrototypeRecordsByQuery() throws IOException {
        Path path = tempDir.resolve("query.tog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path, CompressionAlgorithm.GZIP, 1)) {
            writer.append(new LogRecord(10_000L, LogLevel.INFO, "app", "main", "alpha", null));
            writer.append(new LogRecord(10_030L, LogLevel.INFO, "app", "main", "beta", null));
            writer.append(new LogRecord(10_050L, LogLevel.INFO, "app", "main", "beta-gamma", null));
        }

        LogQuery query = LogQuery.builder()
                .startTimestampMillis(Long.valueOf(10_030L))
                .endTimestampMillis(Long.valueOf(10_050L))
                .minimumLevel(LogLevel.INFO)
                .keyword("beta")
                .build();

        List<LogRecord> records;
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(path)) {
            records = collect(reader.scan(query));
        }

        assertEquals(2, records.size());
        assertTrue(records.get(0).getMessage().contains("beta"));
        assertTrue(records.get(1).getMessage().contains("beta"));
    }

    @Test
    void shouldRoundTripAcrossAllSupportedCompressionAlgorithms() throws IOException {
        for (CompressionAlgorithm algorithm : CompressionAlgorithm.values()) {
            Path path = tempDir.resolve("roundtrip-" + algorithm.getDisplayName() + ".tog");
            try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path, algorithm, 1)) {
                writer.append(new LogRecord(20_000L, LogLevel.INFO, "app", "main", "alpha-beta-gamma", null));
                writer.append(new LogRecord(20_040L, LogLevel.INFO, "app", "main", "delta-epsilon-zeta", null));
            }

            List<LogRecord> records;
            try (PrototypeLogFileReader reader = new PrototypeLogFileReader(path)) {
                records = collect(reader.scan());
            }

            assertEquals(2, records.size(), "record count should round-trip for " + algorithm.getDisplayName());
            assertEquals("alpha-beta-gamma", records.get(0).getMessage());
            assertEquals("delta-epsilon-zeta", records.get(1).getMessage());
        }
    }

    /**
     * Drains one iterator into a stable list for assertions.
     */
    private static List<LogRecord> collect(Iterator<LogRecord> iterator) {
        List<LogRecord> result = new ArrayList<>();
        while (iterator.hasNext()) {
            result.add(iterator.next());
        }
        return Collections.unmodifiableList(result);
    }

    private static byte[] readVersion(byte[] header) {
        return new byte[] {header[0], header[1], header[2]};
    }

    private static int readCompressionAlgorithm(byte[] header) {
        return ((header[3] & 0xFF) << 8) | (header[4] & 0xFF);
    }

    private static int readTrunkSizeKb(byte[] header) {
        return ((header[5] & 0xFF) << 8) | (header[6] & 0xFF);
    }

    private static long readBaseTimestamp(byte[] header) {
        return ((long) (header[7] & 0xFF) << 56)
                | ((long) (header[8] & 0xFF) << 48)
                | ((long) (header[9] & 0xFF) << 40)
                | ((long) (header[10] & 0xFF) << 32)
                | ((long) (header[11] & 0xFF) << 24)
                | ((long) (header[12] & 0xFF) << 16)
                | ((long) (header[13] & 0xFF) << 8)
                | (long) (header[14] & 0xFF);
    }

    private static long readTotalLogLineCount(byte[] header) {
        return ((long) (header[15] & 0xFF) << 56)
                | ((long) (header[16] & 0xFF) << 48)
                | ((long) (header[17] & 0xFF) << 40)
                | ((long) (header[18] & 0xFF) << 32)
                | ((long) (header[19] & 0xFF) << 24)
                | ((long) (header[20] & 0xFF) << 16)
                | ((long) (header[21] & 0xFF) << 8)
                | (long) (header[22] & 0xFF);
    }

    private static int readTrunkCount(byte[] header) {
        return ((header[23] & 0xFF) << 16) | ((header[24] & 0xFF) << 8) | (header[25] & 0xFF);
    }

    private static String repeat(char value, int count) {
        StringBuilder builder = new StringBuilder(count);
        for (int index = 0; index < count; index++) {
            builder.append(value);
        }
        return builder.toString();
    }
}
