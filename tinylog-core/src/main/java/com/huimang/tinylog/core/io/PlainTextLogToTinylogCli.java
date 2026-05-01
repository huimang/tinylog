package com.huimang.tinylog.core.io;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Provides a command-line entrypoint for converting plaintext logs to .tog files.
 */
public final class PlainTextLogToTinylogCli {
    private PlainTextLogToTinylogCli() {
    }

    /**
     * Converts one plaintext log file into one prototype tinylog file.
     */
    public static void main(String[] args) throws IOException {
        if (args.length != 2 && args.length != 3) {
            throw new IllegalArgumentException(
                    "usage: PlainTextLogToTinylogCli <input.log> <output.tog> [algorithmId]");
        }
        Path inputPath = Paths.get(args[0]);
        Path outputPath = Paths.get(args[1]);
        CompressionAlgorithm compressionAlgorithm = args.length == 3
                ? CompressionAlgorithm.fromId(Integer.parseInt(args[2]))
                : PrototypeLogFileFormat.DEFAULT_COMPRESSION_ALGORITHM;
        new PlainTextLogToTinylogConverter(compressionAlgorithm).convert(inputPath, outputPath);
        System.out.println("converted " + inputPath + " to " + outputPath
                + " using " + compressionAlgorithm.getDisplayName());
    }
}
