package com.huimang.tinylog.sdk;

import com.huimang.tinylog.core.io.CompressionAlgorithm;
import com.huimang.tinylog.core.model.LogLevel;
import java.util.Objects;

/**
 * Describes one configured output target.
 */
public final class TinylogAppenderConfiguration {
    /**
     * Supported output types.
     */
    public enum Type {
        CONSOLE,
        FILE
    }

    /**
     * Supported console targets.
     */
    public enum ConsoleTarget {
        SYSTEM_OUT,
        SYSTEM_ERR
    }

    /**
     * Supported file encodings.
     */
    public enum FileFormat {
        TEXT,
        TOG
    }

    private final String name;
    private final Type type;
    private final LogLevel level;
    private final String pattern;
    private final ConsoleTarget consoleTarget;
    private final String fileName;
    private final String filePattern;
    private final FileFormat fileFormat;
    private final CompressionAlgorithm compressionAlgorithm;
    private final int trunkSizeKb;
    private final boolean splitByLevel;
    private final long maxFileSizeBytes;
    private final int maxArchivedFiles;

    /**
     * Creates one immutable appender definition.
     */
    public TinylogAppenderConfiguration(String name,
            Type type,
            LogLevel level,
            String pattern,
            ConsoleTarget consoleTarget,
            String fileName,
            String filePattern,
            FileFormat fileFormat,
            CompressionAlgorithm compressionAlgorithm,
            int trunkSizeKb,
            boolean splitByLevel,
            long maxFileSizeBytes,
            int maxArchivedFiles) {
        this.name = Objects.requireNonNull(name, "name");
        this.type = Objects.requireNonNull(type, "type");
        this.level = Objects.requireNonNull(level, "level");
        this.pattern = Objects.requireNonNull(pattern, "pattern");
        this.consoleTarget = consoleTarget == null ? ConsoleTarget.SYSTEM_OUT : consoleTarget;
        this.fileName = fileName;
        this.filePattern = filePattern;
        this.fileFormat = fileFormat == null ? FileFormat.TEXT : fileFormat;
        this.compressionAlgorithm = compressionAlgorithm == null ? CompressionAlgorithm.GZIP : compressionAlgorithm;
        this.trunkSizeKb = trunkSizeKb <= 0 ? 512 : trunkSizeKb;
        this.splitByLevel = splitByLevel;
        this.maxFileSizeBytes = maxFileSizeBytes;
        this.maxArchivedFiles = maxArchivedFiles;
    }

    public String getName() {
        return name;
    }

    public Type getType() {
        return type;
    }

    public LogLevel getLevel() {
        return level;
    }

    public String getPattern() {
        return pattern;
    }

    public ConsoleTarget getConsoleTarget() {
        return consoleTarget;
    }

    public String getFileName() {
        return fileName;
    }

    public String getFilePattern() {
        return filePattern;
    }

    public FileFormat getFileFormat() {
        return fileFormat;
    }

    public CompressionAlgorithm getCompressionAlgorithm() {
        return compressionAlgorithm;
    }

    public int getTrunkSizeKb() {
        return trunkSizeKb;
    }

    public boolean isSplitByLevel() {
        return splitByLevel;
    }

    public long getMaxFileSizeBytes() {
        return maxFileSizeBytes;
    }

    public int getMaxArchivedFiles() {
        return maxArchivedFiles;
    }
}
