package com.huimang.tinylog.example;

import com.huimang.tinylog.sdk.LogContext;
import com.huimang.tinylog.sdk.Logger;
import com.huimang.tinylog.sdk.LoggerFactory;
import java.net.URISyntaxException;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Generates a large `.tog` fixture through the Java logging pipeline.
 */
public final class TinylogLargeTogExample {
    private static final long DEFAULT_TARGET_SIZE_BYTES = 120L * 1024L * 1024L;
    private static final int PAYLOAD_HEX_CHARS = 768;
    private static final Path OUTPUT_PATH = Paths.get("logs", "tinylog-large-example.tog");

    private TinylogLargeTogExample() {
    }

    /**
     * Generates a `.tog` file until it reaches the requested size. Defaults to roughly 120 MiB.
     */
    public static void main(String[] args) throws Exception {
        long targetSizeBytes = parseTargetSizeBytes(args);
        Path configurationPath = resolveConfigurationPath("tinylog-large.yml");
        Files.createDirectories(OUTPUT_PATH.getParent());
        Files.deleteIfExists(OUTPUT_PATH);
        Files.deleteIfExists(Paths.get(OUTPUT_PATH.toString() + ".buffer"));

        long currentSize = 0L;
        int index = 0;
        try (LoggerFactory factory = LoggerFactory.load(configurationPath)) {
            Logger logger = factory.getLogger(TinylogLargeTogExample.class);
            while (currentSize < targetSizeBytes) {
                long traceId = mix(index);
                LogContext.put("requestId", "REQ-" + index);
                LogContext.put("userId", "user-" + Long.toUnsignedString(traceId));
                String message = "event=" + index
                        + " traceId=" + Long.toUnsignedString(traceId)
                        + " payload=" + buildPayload(traceId)
                        + " email=bulk" + index + "@example.com password=super-secret-" + index;
                switch (index % 5) {
                    case 0:
                        logger.trace(message);
                        break;
                    case 1:
                        logger.debug(message);
                        break;
                    case 2:
                        logger.info(message);
                        break;
                    case 3:
                        logger.warn(message);
                        break;
                    default:
                        logger.error(message);
                        break;
                }
                index++;
                if (index % 1_000 == 0) {
                    currentSize = Files.exists(OUTPUT_PATH) ? Files.size(OUTPUT_PATH) : 0L;
                    System.out.println("progress records=" + index + " output=" + currentSize);
                }
            }
            LogContext.clear();
            logger.info("large tog generation completed records=" + index);
        }

        long finalSize = Files.size(OUTPUT_PATH);
        System.out.println("generated " + OUTPUT_PATH + " size=" + finalSize + " bytes records~" + index);
    }

    private static long parseTargetSizeBytes(String[] args) {
        if (args.length == 0) {
            return DEFAULT_TARGET_SIZE_BYTES;
        }
        return Long.parseLong(args[0]) * 1024L * 1024L;
    }

    private static Path resolveConfigurationPath(String resourceName) throws URISyntaxException {
        URL resource = TinylogLargeTogExample.class.getClassLoader().getResource(resourceName);
        if (resource == null) {
            throw new IllegalStateException("missing resource " + resourceName);
        }
        return Paths.get(resource.toURI());
    }

    private static String buildPayload(long seed) {
        StringBuilder builder = new StringBuilder(PAYLOAD_HEX_CHARS);
        long value = seed;
        while (builder.length() < PAYLOAD_HEX_CHARS) {
            value = mix(value + builder.length());
            builder.append(Long.toHexString(value));
        }
        return builder.substring(0, PAYLOAD_HEX_CHARS);
    }

    private static long mix(long value) {
        long x = value + 0x9E3779B97F4A7C15L;
        x = (x ^ (x >>> 30)) * 0xBF58476D1CE4E5B9L;
        x = (x ^ (x >>> 27)) * 0x94D049BB133111EBL;
        return x ^ (x >>> 31);
    }
}
