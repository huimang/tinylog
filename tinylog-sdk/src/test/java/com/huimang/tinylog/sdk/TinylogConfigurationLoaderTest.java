package com.huimang.tinylog.sdk;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.huimang.tinylog.core.model.LogLevel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;

/**
 * Covers YAML and properties loading for the configurable Java logging module.
 */
class TinylogConfigurationLoaderTest {
    @Test
    void shouldLoadYamlConfiguration() throws Exception {
        Path file = Files.createTempFile("tinylog-config", ".yml");
        Files.write(file, ("" +
                "tinylog:\n" +
                "  root:\n" +
                "    level: trace\n" +
                "    appenders:\n" +
                "      - console\n" +
                "  appenders:\n" +
                "    console:\n" +
                "      type: console\n" +
                "      level: debug\n" +
                "      pattern: \"[%logger] %message\"\n" +
                "  masking:\n" +
                "    contentRules:\n" +
                "      - password\n" +
                "    variableRules:\n" +
                "      requestId: partial\n").getBytes(StandardCharsets.UTF_8));

        TinylogConfiguration configuration = new TinylogConfigurationLoader().load(file);

        assertEquals(LogLevel.TRACE, configuration.getRootLevel());
        assertEquals(1, configuration.getRootAppenders().size());
        assertEquals(LogLevel.DEBUG, configuration.getAppenders().get("console").getLevel());
        assertTrue(configuration.getMasking().getContentRules().contains("password"));
        assertEquals("partial", configuration.getMasking().getVariableRules().get("requestId"));
    }

    @Test
    void shouldLoadPropertiesConfiguration() throws Exception {
        Path file = Files.createTempFile("tinylog-config", ".properties");
        Files.write(file, ("" +
                "tinylog.root.level=info\n" +
                "tinylog.root.appenders=file\n" +
                "tinylog.appender.file.type=file\n" +
                "tinylog.appender.file.level=warn\n" +
                "tinylog.appender.file.fileName=logs/test.log\n" +
                "tinylog.appender.file.splitByLevel=true\n" +
                "tinylog.appender.file.policies.size.size=1MB\n" +
                "tinylog.masking.contentRules=password,email\n" +
                "tinylog.masking.variableRules.userId=partial\n").getBytes(StandardCharsets.UTF_8));

        TinylogConfiguration configuration = new TinylogConfigurationLoader().load(file);

        TinylogAppenderConfiguration fileAppender = configuration.getAppenders().get("file");
        assertEquals(LogLevel.INFO, configuration.getRootLevel());
        assertEquals(LogLevel.WARN, fileAppender.getLevel());
        assertEquals(TinylogAppenderConfiguration.FileFormat.TEXT, fileAppender.getFileFormat());
        assertTrue(fileAppender.isSplitByLevel());
        assertEquals(1024L * 1024L, fileAppender.getMaxFileSizeBytes());
        assertFalse(configuration.getMasking().getContentRules().isEmpty());
    }
}
