package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * Writes the current prototype tinylog file format to a single file path.
 */
public final class PrototypeLogFileWriter implements LogWriter {
    private final Path path;
    private final List<LogRecord> records;
    private boolean closed;

    /**
     * Creates a writer that rewrites the target file on each flush.
     */
    public PrototypeLogFileWriter(Path path) {
        this.path = Objects.requireNonNull(path, "path");
        this.records = new ArrayList<LogRecord>();
    }

    @Override
    public void append(LogRecord record) {
        ensureOpen();
        Objects.requireNonNull(record, "record");
        if (!records.isEmpty()) {
            long previousTimestamp = records.get(records.size() - 1).getTimestampMillis();
            if (record.getTimestampMillis() < previousTimestamp) {
                throw new IllegalArgumentException("records must be appended in timestamp order");
            }
        }
        byte[] content = PrototypeLogFileFormat.toContentBytes(record);
        if (content.length > PrototypeLogFileFormat.MAX_CONTENT_LENGTH) {
            throw new IllegalArgumentException("record content must fit in 3 bytes");
        }
        records.add(record);
    }

    @Override
    public void flush() throws IOException {
        ensureOpen();
        Path parent = path.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        try (DataOutputStream output = new DataOutputStream(
                new BufferedOutputStream(Files.newOutputStream(path)))) {
            long startTimestampMillis = records.isEmpty() ? 0L : records.get(0).getTimestampMillis();
            output.writeLong(startTimestampMillis);
            output.writeLong(records.size());
            for (LogRecord record : records) {
                long offsetMillis = record.getTimestampMillis() - startTimestampMillis;
                if (offsetMillis < 0 || offsetMillis > PrototypeLogFileFormat.MAX_OFFSET_MILLIS) {
                    throw new IllegalArgumentException("record offset must fit in 4 bytes");
                }
                byte[] content = PrototypeLogFileFormat.toContentBytes(record);
                output.writeInt((int) offsetMillis);
                PrototypeLogFileFormat.writeUnsignedMedium(output, content.length);
                output.write(content);
            }
        }
    }

    @Override
    public void close() throws IOException {
        if (!closed) {
            flush();
            closed = true;
        }
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("writer is already closed");
        }
    }
}
