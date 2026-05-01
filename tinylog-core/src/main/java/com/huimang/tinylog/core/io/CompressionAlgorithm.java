package com.huimang.tinylog.core.io;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.Arrays;
import java.util.Objects;
import java.util.zip.DeflaterOutputStream;
import java.util.zip.GZIPInputStream;
import java.util.zip.GZIPOutputStream;
import java.util.zip.InflaterInputStream;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;
import org.apache.commons.compress.compressors.bzip2.BZip2CompressorInputStream;
import org.apache.commons.compress.compressors.bzip2.BZip2CompressorOutputStream;
import org.apache.commons.compress.compressors.snappy.FramedSnappyCompressorInputStream;
import org.apache.commons.compress.compressors.snappy.FramedSnappyCompressorOutputStream;
import org.apache.commons.compress.compressors.xz.XZCompressorInputStream;
import org.apache.commons.compress.compressors.xz.XZCompressorOutputStream;
import org.apache.commons.compress.compressors.zstandard.ZstdCompressorInputStream;
import org.apache.commons.compress.compressors.zstandard.ZstdCompressorOutputStream;

/**
 * Enumerates the prototype line-body compression algorithms supported by tinylog.
 */
public enum CompressionAlgorithm {
    /**
     * Stores the message payload without compression.
     */
    NONE(0, "none") {
        @Override
        byte[] compressBytes(byte[] input) {
            return Arrays.copyOf(input, input.length);
        }

        @Override
        byte[] decompressBytes(byte[] input) {
            return Arrays.copyOf(input, input.length);
        }
    },

    /**
     * Compresses the message payload with GZIP.
     */
    GZIP(1, "gzip") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) throws IOException {
                    return new GZIPOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) throws IOException {
                    return new GZIPInputStream(inputStream);
                }
            });
        }
    },

    /**
     * Compresses the message payload with a single-entry ZIP archive.
     */
    ZIP(2, "zip") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            ByteArrayOutputStream buffer = new ByteArrayOutputStream();
            try (ZipOutputStream output = new ZipOutputStream(buffer)) {
                output.putNextEntry(new ZipEntry("payload"));
                output.write(input);
                output.closeEntry();
            }
            return buffer.toByteArray();
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            ByteArrayInputStream buffer = new ByteArrayInputStream(input);
            try (ZipInputStream zipInput = new ZipInputStream(buffer)) {
                ZipEntry entry = zipInput.getNextEntry();
                if (entry == null) {
                    throw new IOException("zip payload does not contain an entry");
                }
                byte[] result = readAllBytes(zipInput);
                zipInput.closeEntry();
                return result;
            }
        }
    },

    /**
     * Compresses the message payload with raw DEFLATE.
     */
    DEFLATE(3, "deflate") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) {
                    return new DeflaterOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) {
                    return new InflaterInputStream(inputStream);
                }
            });
        }
    },

    /**
     * Compresses the message payload with BZip2.
     */
    BZIP2(4, "bzip2") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) throws IOException {
                    return new BZip2CompressorOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) throws IOException {
                    return new BZip2CompressorInputStream(inputStream);
                }
            });
        }
    },

    /**
     * Compresses the message payload with XZ.
     */
    XZ(5, "xz") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) throws IOException {
                    return new XZCompressorOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) throws IOException {
                    return new XZCompressorInputStream(inputStream);
                }
            });
        }
    },

    /**
     * Compresses the message payload with Zstandard.
     */
    ZSTD(6, "zstd") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) throws IOException {
                    return new ZstdCompressorOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) throws IOException {
                    return new ZstdCompressorInputStream(inputStream);
                }
            });
        }
    },

    /**
     * Compresses the message payload with framed Snappy.
     */
    SNAPPY(7, "snappy") {
        @Override
        byte[] compressBytes(byte[] input) throws IOException {
            return writeCompressed(input, new OutputStreamFactory() {
                @Override
                public OutputStream create(OutputStream output) throws IOException {
                    return new FramedSnappyCompressorOutputStream(output);
                }
            });
        }

        @Override
        byte[] decompressBytes(byte[] input) throws IOException {
            return readCompressed(input, new InputStreamFactory() {
                @Override
                public InputStream create(InputStream inputStream) throws IOException {
                    return new FramedSnappyCompressorInputStream(inputStream);
                }
            });
        }
    };

    /**
     * Stores the persisted algorithm identifier used inside the two-byte header field.
     */
    private final int id;

    /**
     * Stores the stable display name of the algorithm.
     */
    private final String displayName;

    CompressionAlgorithm(int id, String displayName) {
        this.id = id;
        this.displayName = Objects.requireNonNull(displayName, "displayName");
    }

    /**
     * Returns the persisted algorithm identifier used inside the two-byte header field.
     */
    public int getId() {
        return id;
    }

    /**
     * Returns the stable display name of the algorithm.
     */
    public String getDisplayName() {
        return displayName;
    }

    /**
     * Compresses one message payload.
     */
    public final byte[] compress(byte[] input) throws IOException {
        Objects.requireNonNull(input, "input");
        return compressBytes(input);
    }

    /**
     * Decompresses one message payload.
     */
    public final byte[] decompress(byte[] input) throws IOException {
        Objects.requireNonNull(input, "input");
        return decompressBytes(input);
    }

    /**
     * Resolves one persisted header identifier to a known algorithm.
     */
    public static CompressionAlgorithm fromId(int id) {
        for (CompressionAlgorithm algorithm : values()) {
            if (algorithm.id == id) {
                return algorithm;
            }
        }
        throw new IllegalArgumentException("unsupported compression algorithm id: " + id);
    }

    /**
     * Compresses one payload with the algorithm-specific implementation.
     */
    abstract byte[] compressBytes(byte[] input) throws IOException;

    /**
     * Decompresses one payload with the algorithm-specific implementation.
     */
    abstract byte[] decompressBytes(byte[] input) throws IOException;

    /**
     * Writes one compressed payload through the supplied output stream factory.
     */
    private static byte[] writeCompressed(byte[] input, OutputStreamFactory factory) throws IOException {
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        try (OutputStream compressed = factory.create(buffer)) {
            compressed.write(input);
        }
        return buffer.toByteArray();
    }

    /**
     * Reads one compressed payload through the supplied input stream factory.
     */
    private static byte[] readCompressed(byte[] input, InputStreamFactory factory) throws IOException {
        ByteArrayInputStream buffer = new ByteArrayInputStream(input);
        try (InputStream compressed = factory.create(buffer)) {
            return readAllBytes(compressed);
        }
    }

    /**
     * Drains one input stream into a byte array.
     */
    private static byte[] readAllBytes(InputStream inputStream) throws IOException {
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        byte[] chunk = new byte[1024];
        int read;
        while ((read = inputStream.read(chunk)) != -1) {
            buffer.write(chunk, 0, read);
        }
        return buffer.toByteArray();
    }

    /**
     * Creates a compression output stream on demand.
     */
    private interface OutputStreamFactory {
        /**
         * Creates one output stream that writes compressed bytes.
         */
        OutputStream create(OutputStream output) throws IOException;
    }

    /**
     * Creates a decompression input stream on demand.
     */
    private interface InputStreamFactory {
        /**
         * Creates one input stream that reads decompressed bytes.
         */
        InputStream create(InputStream inputStream) throws IOException;
    }

}
