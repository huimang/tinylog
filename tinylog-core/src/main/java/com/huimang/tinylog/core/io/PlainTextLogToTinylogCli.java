package com.huimang.tinylog.core.io;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Provides a command-line entrypoint for converting plaintext logs to trunk-based `.tog` files.
 */
public final class PlainTextLogToTinylogCli {
    private PlainTextLogToTinylogCli() {
    }

    /**
     * Converts one plaintext log file into one tinylog file.
     */
    public static void main(String[] args) throws IOException {
        if (args.length < 2 || args.length > 4) {
            throw new IllegalArgumentException(
                    "usage: PlainTextLogToTinylogCli <input.log> <output.tog> [algorithmId] [trunkSizeKb]");
        }
        Path inputPath = Paths.get(args[0]);
        Path outputPath = Paths.get(args[1]);
        CompressionAlgorithm compressionAlgorithm = args.length >= 3
                ? CompressionAlgorithm.fromId(Integer.parseInt(args[2]))
                : PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM;
        int trunkSizeKb = args.length == 4
                ? Integer.parseInt(args[3])
                : PrototypeLogFileFormat.DEFAULT_TRUNK_SIZE_KB;
        new PlainTextLogToTinylogConverter(compressionAlgorithm, trunkSizeKb).convert(inputPath, outputPath);
        System.out.println("converted " + inputPath + " to " + outputPath
                + " using " + compressionAlgorithm.getDisplayName());
    }
}
