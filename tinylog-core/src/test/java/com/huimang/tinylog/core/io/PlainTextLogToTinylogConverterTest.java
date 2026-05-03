package com.huimang.tinylog.core.io;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class PlainTextLogToTinylogConverterTest {
    @TempDir
    Path tempDir;

    @Test
    void shouldConvertMultilinePlainTextRecords() throws IOException {
        Path inputPath = tempDir.resolve("plain.log");
        Path outputPath = tempDir.resolve("plain.tog");
        Files.write(
                inputPath,
                (
                        "2026-05-01 22:01:00,253 [ERROR] request failed\n"
                                + "java.lang.IllegalStateException: boom\n"
                                + "\tat example.Service.handle(Service.java:42)\n"
                                + "2026-05-01 22:01:00,278 [INFO] recovered\n")
                        .getBytes(PrototypeLogFileFormat.CONTENT_CHARSET));

        new PlainTextLogToTinylogConverter().convert(inputPath, outputPath);

        List<LogRecord> records = new ArrayList<LogRecord>();
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(outputPath)) {
            Iterator<LogRecord> iterator = reader.scan();
            while (iterator.hasNext()) {
                records.add(iterator.next());
            }
        }

        assertEquals(2, records.size());
        assertEquals(
                "request failed\njava.lang.IllegalStateException: boom\n\tat example.Service.handle(Service.java:42)",
                records.get(0).getMessage());
        assertEquals("recovered", records.get(1).getMessage());
    }
}
