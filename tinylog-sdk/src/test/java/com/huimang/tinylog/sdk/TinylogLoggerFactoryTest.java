package com.huimang.tinylog.sdk;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.huimang.tinylog.core.io.PrototypeLogFileReader;
import com.huimang.tinylog.core.model.LogRecord;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Covers file output, masking, rotation, and level-split behavior.
 */
class TinylogLoggerFactoryTest {
    @Test
    void shouldWriteMaskedInfoLogsToConfiguredFile() throws Exception {
        Path directory = Files.createTempDirectory("tinylog-configured-file");
        Path config = directory.resolve("tinylog.properties");
        Path logFile = directory.resolve("application.log");
        Files.write(config, ("" +
                "tinylog.root.level=trace\n" +
                "tinylog.root.appenders=file\n" +
                "tinylog.appender.file.type=file\n" +
                "tinylog.appender.file.level=info\n" +
                "tinylog.appender.file.fileName=" + propertyValue(logFile) + "\n" +
                "tinylog.appender.file.pattern=[%logger] requestId=%var{requestId:-missing} user=%env{TINYLOG_TEST_USER:-guest} %message\n" +
                "tinylog.masking.contentRules=password,mobile,email\n" +
                "tinylog.masking.variableRules.requestId=partial\n").getBytes(StandardCharsets.UTF_8));

        try (LoggerFactory factory = LoggerFactory.load(config)) {
            Logger logger = factory.getLogger("checkout");
            LogContext.put("requestId", "REQ-123456");
            logger.debug("debug should be filtered");
            logger.info("password=secret phone=13812345678 email=tinylog@example.com");
            LogContext.clear();
        }

        String output = new String(Files.readAllBytes(logFile), StandardCharsets.UTF_8);
        assertFalse(output.contains("debug should be filtered"));
        assertTrue(output.contains("[INFO]"));
        assertTrue(output.contains("requestId=RE****56"));
        assertTrue(output.contains("user=guest"));
        assertTrue(output.contains("password=******"));
        assertTrue(output.contains("138****5678"));
        assertTrue(output.contains("t***@example.com"));
    }

    @Test
    void shouldSplitFilesByLevelAndRotateArchives() throws Exception {
        Path directory = Files.createTempDirectory("tinylog-configured-rotate");
        Path config = directory.resolve("tinylog.properties");
        Path logFile = directory.resolve("level.log");
        Path archivePattern = directory.resolve("archive").resolve("level-%level-%i.log");
        Files.write(config, ("" +
                "tinylog.root.level=trace\n" +
                "tinylog.root.appenders=file\n" +
                "tinylog.appender.file.type=file\n" +
                "tinylog.appender.file.level=trace\n" +
                "tinylog.appender.file.fileName=" + propertyValue(logFile) + "\n" +
                "tinylog.appender.file.filePattern=" + propertyValue(archivePattern) + "\n" +
                "tinylog.appender.file.splitByLevel=true\n" +
                "tinylog.appender.file.pattern=[%logger] %message\n" +
                "tinylog.appender.file.policies.size.size=200B\n" +
                "tinylog.appender.file.strategy.max=2\n").getBytes(StandardCharsets.UTF_8));

        try (LoggerFactory factory = LoggerFactory.load(config)) {
            Logger logger = factory.getLogger("inventory");
            for (int index = 0; index < 12; index++) {
                logger.info("info payload block=" + index + " repeated=ABCDEFGHIJKLMNOPQRSTUVWXYZ");
            }
            logger.error("error payload repeated=ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        }

        assertTrue(Files.exists(directory.resolve("level-info.log")));
        assertTrue(Files.exists(directory.resolve("level-error.log")));
        assertTrue(Files.exists(directory.resolve("archive").resolve("level-info-1.log")));
    }

    @Test
    void shouldWriteConfiguredTogFile() throws Exception {
        Path directory = Files.createTempDirectory("tinylog-configured-tog");
        Path config = directory.resolve("tinylog.properties");
        Path togFile = directory.resolve("application.tog");
        Files.write(config, ("" +
                "tinylog.root.level=trace\n" +
                "tinylog.root.appenders=file\n" +
                "tinylog.appender.file.type=file\n" +
                "tinylog.appender.file.level=info\n" +
                "tinylog.appender.file.fileName=" + propertyValue(togFile) + "\n" +
                "tinylog.appender.file.format=tog\n" +
                "tinylog.appender.file.pattern=[%logger] requestId=%var{requestId:-missing} %message\n" +
                "tinylog.masking.contentRules=password,email\n" +
                "tinylog.masking.variableRules.requestId=partial\n").getBytes(StandardCharsets.UTF_8));

        try (LoggerFactory factory = LoggerFactory.load(config)) {
            Logger logger = factory.getLogger("checkout");
            LogContext.put("requestId", "REQ-123456");
            logger.debug("debug should be filtered");
            logger.info("password=secret email=tinylog@example.com");
            logger.warn("inventory warning");
            LogContext.clear();
        }

        List<LogRecord> records = new ArrayList<LogRecord>();
        try (PrototypeLogFileReader reader = new PrototypeLogFileReader(togFile)) {
            Iterator<LogRecord> iterator = reader.scan();
            while (iterator.hasNext()) {
                records.add(iterator.next());
            }
        }

        assertEquals(2, records.size());
        assertTrue(records.get(0).getMessage().contains("requestId=RE****56"));
        assertTrue(records.get(0).getMessage().contains("password=******"));
        assertTrue(records.get(0).getMessage().contains("t***@example.com"));
        assertTrue(records.get(1).getMessage().contains("inventory warning"));
    }

    private static String propertyValue(Path path) {
        return path.toString().replace("\\", "\\\\");
    }
}
