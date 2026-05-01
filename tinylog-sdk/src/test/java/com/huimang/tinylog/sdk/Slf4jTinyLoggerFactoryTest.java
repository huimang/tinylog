package com.huimang.tinylog.sdk;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;
import org.slf4j.ILoggerFactory;
import org.slf4j.LoggerFactory;

public class Slf4jTinyLoggerFactoryTest {
    @Test
    void shouldCreateTinyLoggerBackedBySlf4jApi() {
        TinyLogger logger = new Slf4jTinyLoggerFactory().getLogger(Slf4jTinyLoggerFactoryTest.class);

        assertInstanceOf(Slf4jTinyLogger.class, logger);
        assertEquals(Slf4jTinyLoggerFactoryTest.class.getName(), logger.getName());
    }

    @Test
    void shouldResolveSl4fjSimpleDuringTests() {
        ILoggerFactory factory = LoggerFactory.getILoggerFactory();

        assertTrue(factory.getClass().getName().contains("SimpleLoggerFactory"));
    }

    @Test
    void shouldDelegateLoggingCallsToSlf4j() {
        Slf4jTinyLogger logger =
                (Slf4jTinyLogger) new Slf4jTinyLoggerFactory().getLogger("tinylog.sdk.slf4j");

        logger.trace("trace message");
        logger.debug("debug message");
        logger.info("info message");
        logger.warn("warn message");
        logger.error("error message");
        logger.error("error with cause", new IllegalStateException("boom"));

        assertEquals("tinylog.sdk.slf4j", logger.getDelegate().getName());
    }
}
