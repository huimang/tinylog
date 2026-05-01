package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.BufferedInputStream;
import java.io.DataInputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.Objects;

/**
 * Reads the current prototype tinylog file format from a single file path.
 */
public final class PrototypeLogFileReader implements LogReader {
    private final Path path;
    private boolean closed;

    /**
     * Creates a reader for one prototype log file.
     */
    public PrototypeLogFileReader(Path path) {
        this.path = Objects.requireNonNull(path, "path");
    }

    @Override
    public Iterator<LogRecord> scan() throws IOException {
        ensureOpen();
        return new PrototypeLogIterator(path, null);
    }

    @Override
    public Iterator<LogRecord> scan(LogQuery query) throws IOException {
        ensureOpen();
        return new PrototypeLogIterator(path, Objects.requireNonNull(query, "query"));
    }

    @Override
    public void close() {
        closed = true;
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("reader is already closed");
        }
    }

    /**
     * Streams and filters records lazily while hiding low-level file parsing details.
     */
    private static final class PrototypeLogIterator implements Iterator<LogRecord> {
        private final DataInputStream input;
        private final LogQuery query;
        private final long startTimestampMillis;
        private long remainingEntries;
        private LogRecord nextRecord;
        private boolean prepared;
        private boolean exhausted;

        /**
         * Opens one file stream and reads the prototype header.
         */
        private PrototypeLogIterator(Path path, LogQuery query) throws IOException {
            this.input = new DataInputStream(new BufferedInputStream(Files.newInputStream(path)));
            this.query = query;
            try {
                this.startTimestampMillis = input.readLong();
                this.remainingEntries = input.readLong();
                if (remainingEntries < 0L) {
                    throw new IOException("record count must not be negative");
                }
            } catch (IOException exception) {
                closeQuietly();
                throw exception;
            }
        }

        @Override
        public boolean hasNext() {
            prepare();
            return !exhausted;
        }

        @Override
        public LogRecord next() {
            prepare();
            if (exhausted) {
                throw new NoSuchElementException("no more log records");
            }
            prepared = false;
            return nextRecord;
        }

        /**
         * Advances until a matching record is found or the stream is exhausted.
         */
        private void prepare() {
            if (prepared || exhausted) {
                return;
            }
            prepared = true;
            nextRecord = null;
            try {
                while (remainingEntries > 0L) {
                    long offsetMillis = PrototypeLogFileFormat.readUnsignedInt(input);
                    int contentLength = PrototypeLogFileFormat.readUnsignedMedium(input);
                    byte[] contentBytes = new byte[contentLength];
                    input.readFully(contentBytes);
                    remainingEntries--;
                    LogRecord record = PrototypeLogFileFormat.toRecord(
                            startTimestampMillis,
                            offsetMillis,
                            new String(contentBytes, PrototypeLogFileFormat.CONTENT_CHARSET));
                    if (matches(record)) {
                        nextRecord = record;
                        return;
                    }
                }
                exhausted = true;
                closeQuietly();
            } catch (EOFException exception) {
                exhausted = true;
                closeQuietly();
                throw new UncheckedIOException(new IOException("prototype log file is truncated", exception));
            } catch (IOException exception) {
                exhausted = true;
                closeQuietly();
                throw new UncheckedIOException(exception);
            }
        }

        /**
         * Applies the current query constraints to one record.
         */
        private boolean matches(LogRecord record) {
            if (query == null) {
                return true;
            }
            if (query.getStartTimestampMillis() != null
                    && record.getTimestampMillis() < query.getStartTimestampMillis().longValue()) {
                return false;
            }
            if (query.getEndTimestampMillis() != null
                    && record.getTimestampMillis() > query.getEndTimestampMillis().longValue()) {
                return false;
            }
            if (query.getMinimumLevel() != null
                    && record.getLevel().ordinal() < query.getMinimumLevel().ordinal()) {
                return false;
            }
            if (query.getKeyword() != null
                    && !query.getKeyword().isEmpty()
                    && !record.getMessage().contains(query.getKeyword())) {
                return false;
            }
            return true;
        }

        /**
         * Closes the current stream without surfacing secondary close failures.
         */
        private void closeQuietly() {
            try {
                input.close();
            } catch (IOException ignored) {
                // Nothing to do when closing an exhausted iterator.
            }
        }
    }
}
