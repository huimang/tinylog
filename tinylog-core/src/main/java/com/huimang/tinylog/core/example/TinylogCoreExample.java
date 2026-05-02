package com.huimang.tinylog.core.example;

import com.huimang.tinylog.core.io.PrototypeLogFileReader;
import com.huimang.tinylog.core.io.PrototypeLogFileWriter;
import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Iterator;

/**
 * Demonstrates the basic TinyLog Core write and read flow through language-neutral core contracts.
 */
public final class TinylogCoreExample {
    private TinylogCoreExample() {
    }

    /**
     * Writes two sample records to one `.tog` file and reads them back.
     */
    public static void main(String[] args) throws IOException {
        Path outputPath = args.length == 0 ? Paths.get("example.tog") : Paths.get(args[0]);
        writeExampleFile(outputPath);
        readExampleFile(outputPath);
    }

    private static void writeExampleFile(Path outputPath) throws IOException {
        try (PrototypeLogFileWriter writer = new PrototypeLogFileWriter(outputPath)) {
            writer.append(new LogRecord(
                    1_777_672_860_253L,
                    LogLevel.INFO,
                    "order-service",
                    "startup",
                    "service started",
                    null));
            writer.append(new LogRecord(
                    1_777_672_860_278L,
                    LogLevel.WARN,
                    "order-service",
                    "queue-monitor",
                    "queue depth is rising",
                    null));
        }
    }

    private static void readExampleFile(Path outputPath) throws IOException {
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(outputPath)) {
            Iterator<LogRecord> iterator = reader.scan();
            while (iterator.hasNext()) {
                LogRecord record = iterator.next();
                System.out.println(record.getTimestampMillis()
                        + " [" + record.getLevel() + "] "
                        + record.getMessage());
            }
        }
    }
}
