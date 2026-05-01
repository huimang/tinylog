package com.huimang.tinylong.core.io;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.huimang.tinylong.core.model.LogLevel;
import com.huimang.tinylong.core.model.LogQuery;
import com.huimang.tinylong.core.model.LogRecord;
import java.io.IOException;
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
        Path path = tempDir.resolve("sample.tlog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path)) {
            writer.append(new LogRecord(1_700_000_000_000L, LogLevel.INFO, "app", "main", "alpha", null));
            writer.append(new LogRecord(1_700_000_000_125L, LogLevel.ERROR, "app", "main", "beta", null));
        }

        List<LogRecord> records;
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(path)) {
            records = collect(reader.scan());
        }

        assertEquals(2, records.size());
        assertEquals(1_700_000_000_000L, records.get(0).getTimestampMillis());
        assertEquals(1_700_000_000_125L, records.get(1).getTimestampMillis());
        assertEquals("alpha", records.get(0).getMessage());
        assertEquals("beta", records.get(1).getMessage());
        assertEquals("prototype", records.get(0).getLoggerName());
    }

    @Test
    void shouldFilterPrototypeRecordsByQuery() throws IOException {
        Path path = tempDir.resolve("query.tlog");
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(path)) {
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
}
