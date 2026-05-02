package com.huimang.tinylog.core.io;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Provides a command-line entrypoint for converting plaintext logs to trunk-based `.tog` files.
 */
public final class PlainTextLogToTinylogCli {
    private static final int MIN_ARGUMENT_COUNT = 2;
    private static final int MAX_ARGUMENT_COUNT = 4;
    private static final int ALGORITHM_ID_INDEX = 2;
    private static final int TRUNK_SIZE_INDEX = 3;

    private PlainTextLogToTinylogCli() {
    }

    /**
     * Converts one plaintext log file into one tinylog file.
     */
    public static void main(String[] args) throws IOException {
        if (args.length < MIN_ARGUMENT_COUNT || args.length > MAX_ARGUMENT_COUNT) {
            throw new IllegalArgumentException(
                    "usage: PlainTextLogToTinylogCli <input.log> <output.tog> [algorithmId] [trunkSizeKb]");
        }
        Path inputPath = Paths.get(args[0]);
        Path outputPath = Paths.get(args[1]);
        CompressionAlgorithm compressionAlgorithm = parseCompressionAlgorithm(args);
        int trunkSizeKb = parseTrunkSizeKb(args);
        new PlainTextLogToTinylogConverter(compressionAlgorithm, trunkSizeKb).convert(inputPath, outputPath);
        System.out.println("converted " + inputPath + " to " + outputPath
                + " using " + compressionAlgorithm.getDisplayName());
    }

    private static CompressionAlgorithm parseCompressionAlgorithm(String[] args) {
        if (args.length < ALGORITHM_ID_INDEX + 1) {
            return PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM;
        }
        return CompressionAlgorithm.fromId(Integer.parseInt(args[ALGORITHM_ID_INDEX]));
    }

    private static int parseTrunkSizeKb(String[] args) {
        if (args.length < TRUNK_SIZE_INDEX + 1) {
            return PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB;
        }
        return Integer.parseInt(args[TRUNK_SIZE_INDEX]);
    }
}
