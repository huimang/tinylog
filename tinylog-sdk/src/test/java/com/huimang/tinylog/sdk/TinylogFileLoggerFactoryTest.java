package com.huimang.tinylog.sdk;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.huimang.tinylog.core.io.PrototypeLogFileReader;
import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class TinylogFileLoggerFactoryTest {
    @TempDir
    Path tempDir;

    @Test
    void shouldCreateFileBackedLogger() throws IOException {
        try (TinylogFileLoggerFactory factory = new TinylogFileLoggerFactory(tempDir.resolve("app.tog"))) {
            Logger logger = factory.getLogger("tinylog.sdk.file");

            assertInstanceOf(TinylogFileLogger.class, logger);
            assertEquals("tinylog.sdk.file", logger.getName());
        }
    }

    @Test
    void shouldPersistSdkLogsIntoTinylogFile() throws IOException {
        Path path = tempDir.resolve("sdk.tog");
        try (TinylogFileLoggerFactory factory = new TinylogFileLoggerFactory(path)) {
            Logger logger = factory.getLogger("tinylog.sdk.file");
            logger.info("service started");
            logger.error("request failed", new IllegalStateException("boom"));
        }

        List<LogRecord> records = new ArrayList<LogRecord>();
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(path)) {
            Iterator<LogRecord> iterator = reader.scan();
            while (iterator.hasNext()) {
                records.add(iterator.next());
            }
        }

        assertEquals(2, records.size());
        assertEquals(LogLevel.INFO, records.get(0).getLevel());
        assertEquals(LogLevel.ERROR, records.get(1).getLevel());
        assertEquals("service started", records.get(0).getMessage());
        assertTrue(records.get(1).getMessage().startsWith("request failed\njava.lang.IllegalStateException: boom"));
    }
}
