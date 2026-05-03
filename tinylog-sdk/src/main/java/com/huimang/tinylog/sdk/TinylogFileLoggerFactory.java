package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.io.CompressionAlgorithm;
import com.huimang.tinylog.core.io.PrototypeLogFileWriter;
import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.Closeable;
import java.io.IOException;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.file.Path;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Creates SDK loggers that persist directly into one TinyLog `.tog` file.
 */
public final class TinylogFileLoggerFactory implements TinyLoggerFactory, Closeable {
    private static final int DEFAULT_TRUNK_SIZE_KB = 512;

    private final Map<String, Logger> loggers = new ConcurrentHashMap<String, Logger>();
    private final PrototypeLogFileWriter writer;
    private volatile boolean closed;

    /**
     * Creates a factory that writes to one target file with the default compression and trunk size.
     */
    public TinylogFileLoggerFactory(Path path) throws IOException {
        this(path, CompressionAlgorithm.GZIP, DEFAULT_TRUNK_SIZE_KB);
    }

    /**
     * Creates a factory that writes to one target file with the selected compression and trunk size.
     */
    public TinylogFileLoggerFactory(Path path, CompressionAlgorithm compressionAlgorithm, int trunkSizeKb)
            throws IOException {
        this.writer = new PrototypeLogFileWriter(path, compressionAlgorithm, trunkSizeKb);
    }

    @Override
    public Logger getLogger(String name) {
        ensureOpen();
        Objects.requireNonNull(name, "name");
        Logger existing = loggers.get(name);
        if (existing != null) {
            return existing;
        }
        Logger created = new TinylogFileLogger(name, this);
        Logger raced = loggers.putIfAbsent(name, created);
        return raced == null ? created : raced;
    }

    @Override
    public void close() throws IOException {
        closed = true;
        writer.close();
    }

    void append(String loggerName, LogLevel level, String message, Throwable throwable) {
        ensureOpen();
        Objects.requireNonNull(loggerName, "loggerName");
        Objects.requireNonNull(level, "level");
        Objects.requireNonNull(message, "message");
        LogRecord record = new LogRecord(
                System.currentTimeMillis(),
                level,
                loggerName,
                Thread.currentThread().getName(),
                formatMessage(message, throwable),
                null);
        synchronized (writer) {
            try {
                writer.append(record);
            } catch (IOException exception) {
                throw new IllegalStateException("failed to write tinylog record", exception);
            }
        }
    }

    private String formatMessage(String message, Throwable throwable) {
        if (throwable == null) {
            return message;
        }
        StringWriter buffer = new StringWriter();
        PrintWriter output = new PrintWriter(buffer);
        output.print(message);
        output.println();
        throwable.printStackTrace(output);
        output.flush();
        return trimTrailingLineBreaks(buffer.toString());
    }

    private String trimTrailingLineBreaks(String value) {
        int end = value.length();
        while (end > 0) {
            char current = value.charAt(end - 1);
            if (current != '\n' && current != '\r') {
                break;
            }
            end--;
        }
        return value.substring(0, end);
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("tinylog file logger factory is already closed");
        }
    }
}
