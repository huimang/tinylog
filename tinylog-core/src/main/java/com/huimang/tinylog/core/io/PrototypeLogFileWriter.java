package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Objects;

/**
 * Writes the current trunk-based tinylog file format to a single target file.
 */
public final class PrototypeLogFileWriter implements LogWriter {
    private static final String MAIN_FILE_MODE = "rw";

    private final CompressionAlgorithm compressionAlgorithm;
    private final int trunkSizeBytes;
    private final Path bufferPath;
    private final RandomAccessFile mainFile;
    private final RandomAccessFile bufferFile;
    private final ByteArrayOutputStream rawTrunkBuffer;
    private final DataOutputStream rawTrunkOutput;
    private final Thread shutdownHook;
    private boolean closed;
    private long baseTimestampUtcMillis = -1L;
    private long lastTimestampUtcMillis = Long.MIN_VALUE;
    private long totalLogLineCount;
    private int trunkCount;
    private int currentTrunkBytes;
    private int currentTrunkLineCount;

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
        Objects.requireNonNull(path, "path");
        this.compressionAlgorithm = Objects.requireNonNull(compressionAlgorithm, "compressionAlgorithm");
        int validatedTrunkSizeKb = PrototypeLogFileFormat.validateTrunkSizeKb(trunkSizeKb);
        this.trunkSizeBytes = PrototypeLogFileFormat.trunkSizeBytes(validatedTrunkSizeKb);
        Path parent = path.getParent();
        if (parent != null) {
            Files.createDirectories(parent);
        }
        this.bufferPath = PrototypeLogFileFormat.bufferFilePath(path);
        this.mainFile = new RandomAccessFile(path.toFile(), MAIN_FILE_MODE);
        this.bufferFile = new RandomAccessFile(bufferPath.toFile(), MAIN_FILE_MODE);
        this.rawTrunkBuffer = new ByteArrayOutputStream(trunkSizeBytes);
        this.rawTrunkOutput = new DataOutputStream(rawTrunkBuffer);
        this.mainFile.setLength(0L);
        this.bufferFile.setLength(0L);
        PrototypeLogFileFormat.writeHeader(
                mainFile,
                compressionAlgorithm,
                validatedTrunkSizeKb,
                0L,
                0L,
                0);
        this.shutdownHook = new Thread(new Runnable() {
            @Override
            public void run() {
                closeQuietlyOnShutdown();
            }
        }, "tinylog-prototype-writer-shutdown");
        Runtime.getRuntime().addShutdownHook(shutdownHook);
    }

    @Override
    public void append(LogRecord record) throws IOException {
        ensureOpen();
        Objects.requireNonNull(record, "record");
        ensureTimestampOrder(record);
        initializeBaseTimestamp(record);
        if (currentTrunkLineCount == PrototypeLogFileFormat.MAX_TRUNK_LOG_LINE_COUNT) {
            flushCurrentTrunk();
        }
        int lineBytes = PrototypeLogFileFormat.measureRawLogLine(record);
        if (wouldExceedTrunkSize(lineBytes)) {
            flushCurrentTrunk();
        }
        PrototypeLogFileFormat.writeRawLogLine(rawTrunkOutput, record, baseTimestampUtcMillis);
        appendBufferedRecord(record);
        currentTrunkBytes += lineBytes;
        currentTrunkLineCount++;
        lastTimestampUtcMillis = record.getTimestampMillis();
        if (currentTrunkBytes >= trunkSizeBytes) {
            flushCurrentTrunk();
        }
    }

    @Override
    public void flush() throws IOException {
        ensureOpen();
        rawTrunkOutput.flush();
        flushCurrentTrunk();
    }

    @Override
    public void close() throws IOException {
        if (closed) {
            return;
        }
        try {
            rawTrunkOutput.flush();
            flushCurrentTrunk();
        } finally {
            closed = true;
            rawTrunkOutput.close();
            bufferFile.close();
            mainFile.close();
            unregisterShutdownHook();
        }
    }

    /**
     * Persists the current raw buffer as one compressed trunk.
     */
    private void flushCurrentTrunk() throws IOException {
        if (currentTrunkLineCount == 0) {
            return;
        }
        byte[] rawTrunkBytes = rawTrunkBuffer.toByteArray();
        byte[] compressedTrunkBytes = compressionAlgorithm.compress(rawTrunkBytes);
        mainFile.seek(mainFile.length());
        mainFile.writeShort(currentTrunkLineCount);
        mainFile.writeInt(compressedTrunkBytes.length);
        mainFile.write(compressedTrunkBytes);
        totalLogLineCount += currentTrunkLineCount;
        trunkCount++;
        updateHeaderCounters();
        clearBufferFile();
        rawTrunkBuffer.reset();
        currentTrunkBytes = 0;
        currentTrunkLineCount = 0;
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

    private void appendBufferedRecord(LogRecord record) throws IOException {
        if (bufferFile.length() == 0L) {
            bufferFile.seek(0L);
            bufferFile.writeLong(baseTimestampUtcMillis);
        } else {
            bufferFile.seek(bufferFile.length());
        }
        PrototypeLogFileFormat.writeRawLogLine(bufferFile, record, baseTimestampUtcMillis);
    }

    private void clearBufferFile() throws IOException {
        bufferFile.setLength(0L);
        bufferFile.seek(0L);
    }

    private boolean wouldExceedTrunkSize(int lineBytes) {
        return currentTrunkLineCount > 0 && currentTrunkBytes + lineBytes > trunkSizeBytes;
    }

    private void ensureOpen() {
        if (closed) {
            throw new IllegalStateException("writer is already closed");
        }
    }

    private void unregisterShutdownHook() {
        try {
            Runtime.getRuntime().removeShutdownHook(shutdownHook);
        } catch (IllegalStateException ignored) {
            // JVM shutdown is already in progress.
        }
    }

    private void closeQuietlyOnShutdown() {
        try {
            close();
        } catch (IOException ignored) {
            // Best-effort flush during shutdown.
        }
    }
}
