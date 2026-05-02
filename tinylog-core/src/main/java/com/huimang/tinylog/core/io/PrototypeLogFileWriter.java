package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Objects;

/**
 * Writes the current trunk-based tinylog file format to a single target file.
 */
public final class PrototypeLogFileWriter implements LogWriter {
    private static final String MAIN_FILE_MODE = "rw";
    private static final String BUFFER_FILE_PREFIX = "log-buffer-";
    private static final String BUFFER_FILE_SUFFIX = ".tmp";

    private final Path path;
    private final CompressionAlgorithm compressionAlgorithm;
    private final int trunkSizeKb;
    private final int trunkSizeBytes;
    private final RandomAccessFile mainFile;
    private boolean closed;
    private long baseTimestampUtcMillis = -1L;
    private long lastTimestampUtcMillis = Long.MIN_VALUE;
    private long totalLogLineCount;
    private int trunkCount;
    private int nextTrunkIndex;
    private Path currentBufferPath;
    private DataOutputStream currentBufferOutput;
    private int currentBufferBytes;
    private int currentBufferLineCount;

    /**
     * Creates a writer that uses the default compression and trunk size.
     */
    public PrototypeLogFileWriter(Path path) throws IOException {
        this(path, PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM, PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB);
    }

    /**
     * Creates a writer that uses the selected compression and the default trunk size.
     */
    public PrototypeLogFileWriter(Path path, CompressionAlgorithm compressionAlgorithm) throws IOException {
        this(path, compressionAlgorithm, PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB);
    }

    /**
     * Creates a writer that uses the selected compression and trunk size.
     */
    public PrototypeLogFileWriter(Path path, CompressionAlgorithm compressionAlgorithm, int trunkSizeKb)
            throws IOException {
        this.path = Objects.requireNonNull(path, "path");
        this.compressionAlgorithm = Objects.requireNonNull(compressionAlgorithm, "compressionAlgorithm");
        this.trunkSizeKb = PrototypeLogFileFormat.validateTrunkSizeKb(trunkSizeKb);
        this.trunkSizeBytes = PrototypeLogFileFormat.trunkSizeBytes(this.trunkSizeKb);
        Path parent = path.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        this.mainFile = new RandomAccessFile(path.toFile(), MAIN_FILE_MODE);
        this.mainFile.setLength(0L);
        PrototypeLogFileFormat.writeHeader(
                mainFile,
                compressionAlgorithm,
                this.trunkSizeKb,
                0L,
                0L,
                0);
        openNextBuffer();
    }

    @Override
    public void append(LogRecord record) throws IOException {
        ensureOpen();
        Objects.requireNonNull(record, "record");
        ensureTimestampOrder(record);
        initializeBaseTimestamp(record);
        if (currentBufferLineCount == PrototypeLogFileFormat.MAX_TRUNK_LOG_LINE_COUNT) {
            flushCurrentTrunk(true);
        }
        int lineBytes = PrototypeLogFileFormat.measureRawLogLine(record);
        if (wouldExceedTrunkSize(lineBytes)) {
            flushCurrentTrunk(true);
        }
        PrototypeLogFileFormat.writeRawLogLine(currentBufferOutput, record, baseTimestampUtcMillis);
        currentBufferBytes += lineBytes;
        currentBufferLineCount++;
        lastTimestampUtcMillis = record.getTimestampMillis();
        if (currentBufferBytes >= trunkSizeBytes) {
            flushCurrentTrunk(true);
        }
    }

    @Override
    public void flush() throws IOException {
        ensureOpen();
        if (currentBufferOutput != null) {
            currentBufferOutput.flush();
        }
        flushCurrentTrunk(true);
    }

    @Override
    public void close() throws IOException {
        if (closed) {
            return;
        }
        try {
            if (currentBufferOutput != null) {
                currentBufferOutput.flush();
            }
            flushCurrentTrunk(false);
            closeAndDeleteEmptyBuffer();
        } finally {
            closed = true;
            mainFile.close();
        }
    }

    /**
     * Opens the next raw trunk buffer file.
     */
    private void openNextBuffer() throws IOException {
        currentBufferPath = resolveBufferPath(nextTrunkIndex);
        Files.deleteIfExists(currentBufferPath);
        currentBufferOutput =
                new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(currentBufferPath)));
        currentBufferBytes = 0;
        currentBufferLineCount = 0;
    }

    /**
     * Persists the current raw buffer as one compressed trunk.
     */
    private void flushCurrentTrunk(boolean openNextBuffer) throws IOException {
        if (currentBufferLineCount == 0) {
            if (openNextBuffer && currentBufferOutput == null) {
                openNextBuffer();
            }
            return;
        }
        currentBufferOutput.close();
        currentBufferOutput = null;
        byte[] rawTrunkBytes = Files.readAllBytes(currentBufferPath);
        byte[] compressedTrunkBytes = compressionAlgorithm.compress(rawTrunkBytes);
        mainFile.seek(mainFile.length());
        mainFile.writeShort(currentBufferLineCount);
        mainFile.writeInt(compressedTrunkBytes.length);
        mainFile.write(compressedTrunkBytes);
        totalLogLineCount += currentBufferLineCount;
        trunkCount++;
        updateHeaderCounters();
        Files.deleteIfExists(currentBufferPath);
        nextTrunkIndex++;
        currentBufferBytes = 0;
        currentBufferLineCount = 0;
        currentBufferPath = null;
        if (openNextBuffer) {
            openNextBuffer();
        }
    }

    /**
     * Updates the stored base timestamp once the first record is known.
     */
    private void updateBaseTimestamp() throws IOException {
        mainFile.seek(PrototypeLogFileFormat.BASE_TIMESTAMP_OFFSET);
        mainFile.writeLong(baseTimestampUtcMillis);
        mainFile.seek(mainFile.length());
    }

    /**
     * Updates the header counters after a trunk flush.
     */
    private void updateHeaderCounters() throws IOException {
        mainFile.seek(PrototypeLogFileFormat.TOTAL_LOG_LINE_COUNT_OFFSET);
        mainFile.writeLong(totalLogLineCount);
        PrototypeLogFileFormat.writeUnsignedMedium(mainFile, trunkCount);
        mainFile.seek(mainFile.length());
    }

    /**
     * Closes and removes the empty current buffer file.
     */
    private void closeAndDeleteEmptyBuffer() throws IOException {
        if (currentBufferOutput != null) {
            currentBufferOutput.close();
            currentBufferOutput = null;
        }
        if (currentBufferPath != null) {
            Files.deleteIfExists(currentBufferPath);
            currentBufferPath = null;
        }
    }

    /**
     * Resolves the temporary buffer file path for one trunk index.
     */
    private Path resolveBufferPath(int trunkIndex) {
        String fileName = BUFFER_FILE_PREFIX + trunkIndex + BUFFER_FILE_SUFFIX;
        Path parent = path.getParent();
        return parent == null ? Paths.get(fileName) : parent.resolve(fileName);
    }

    private void ensureTimestampOrder(LogRecord record) {
        if (lastTimestampUtcMillis != Long.MIN_VALUE && record.getTimestampMillis() < lastTimestampUtcMillis) {
            throw new IllegalArgumentException("records must be appended in timestamp order");
        }
    }

    private void initializeBaseTimestamp(LogRecord record) throws IOException {
        if (baseTimestampUtcMillis >= 0L) {
            return;
        }
        baseTimestampUtcMillis = record.getTimestampMillis();
        updateBaseTimestamp();
    }

    private boolean wouldExceedTrunkSize(int lineBytes) {
        return currentBufferLineCount > 0 && currentBufferBytes + lineBytes > trunkSizeBytes;
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("writer is already closed");
        }
    }
}
