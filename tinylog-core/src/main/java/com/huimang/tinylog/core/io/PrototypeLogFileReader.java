package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Objects;

/**
 * Reads the current trunk-based tinylog file format from a single file path.
 */
public final class PrototypeLogFileReader implements LogReader {
    private final Path path;
    private boolean closed;

    /**
     * Creates a reader for one trunk-based tinylog file.
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
     * Streams records trunk by trunk while hiding low-level file parsing details.
     */
    private static final class PrototypeLogIterator implements Iterator<LogRecord> {
        private final DataInputStream input;
        private final CompressionAlgorithm compressionAlgorithm;
        private final LogQuery query;
        private final long baseTimestampUtcMillis;
        private int remainingTrunks;
        private List<LogRecord> currentTrunkRecords;
        private int currentTrunkIndex;
        private LogRecord nextRecord;
        private boolean prepared;
        private boolean exhausted;

        /**
         * Opens one file stream and reads the trunk-based header.
         */
        private PrototypeLogIterator(Path path, LogQuery query) throws IOException {
            this.input = new DataInputStream(new BufferedInputStream(Files.newInputStream(path)));
            this.query = query;
            this.currentTrunkRecords = java.util.Collections.<LogRecord>emptyList();
            try {
                byte[] version = new byte[3];
                input.readFully(version);
                this.compressionAlgorithm = CompressionAlgorithm.fromId(input.readUnsignedShort());
                PrototypeLogFileFormat.validateTrunkSizeKb(input.readUnsignedShort());
                this.baseTimestampUtcMillis = input.readLong();
                long totalLogLineCount = input.readLong();
                if (totalLogLineCount < 0L) {
                    throw new IOException("total log line count must not be negative");
                }
                this.remainingTrunks = PrototypeLogFileFormat.readUnsignedMedium(input);
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
         * Advances until a matching record is found or the file is exhausted.
         */
        private void prepare() {
            if (prepared || exhausted) {
                return;
            }
            prepared = true;
            nextRecord = null;
            try {
                while (true) {
                    if (currentTrunkIndex < currentTrunkRecords.size()) {
                        LogRecord candidate = currentTrunkRecords.get(currentTrunkIndex++);
                        if (matches(candidate)) {
                            nextRecord = candidate;
                            return;
                        }
                        continue;
                    }
                    if (remainingTrunks == 0) {
                        exhausted = true;
                        closeQuietly();
                        return;
                    }
                    loadNextTrunk();
                }
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
         * Loads and decompresses the next trunk from the stream.
         */
        private void loadNextTrunk() throws IOException {
            int trunkLogLineCount = input.readUnsignedShort();
            int compressedContentLength = input.readInt();
            if (compressedContentLength < 0) {
                throw new IOException("compressed trunk length must not be negative");
            }
            byte[] compressedContent = new byte[compressedContentLength];
            input.readFully(compressedContent);
            remainingTrunks--;
            byte[] rawTrunkBytes = compressionAlgorithm.decompress(compressedContent);
            currentTrunkRecords = parseRawTrunk(rawTrunkBytes, trunkLogLineCount);
            currentTrunkIndex = 0;
        }

        /**
         * Parses one decompressed raw trunk payload into logical records.
         */
        private List<LogRecord> parseRawTrunk(byte[] rawTrunkBytes, int trunkLogLineCount) throws IOException {
            DataInputStream trunkInput = new DataInputStream(new ByteArrayInputStream(rawTrunkBytes));
            List<LogRecord> records = new ArrayList<LogRecord>(trunkLogLineCount);
            for (int index = 0; index < trunkLogLineCount; index++) {
                long offsetMillis = PrototypeLogFileFormat.readUnsignedInt(trunkInput);
                int contentLength = trunkInput.readInt();
                if (contentLength < 0) {
                    throw new IOException("raw log line content length must not be negative");
                }
                byte[] contentBytes = new byte[contentLength];
                trunkInput.readFully(contentBytes);
                records.add(PrototypeLogFileFormat.toRecord(
                        baseTimestampUtcMillis,
                        offsetMillis,
                        new String(contentBytes, PrototypeLogFileFormat.CONTENT_CHARSET)));
            }
            if (trunkInput.available() != 0) {
                throw new IOException("raw trunk payload contains trailing bytes");
            }
            return java.util.Collections.unmodifiableList(records);
        }

        /**
         * Applies the current query constraints to one logical record.
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
