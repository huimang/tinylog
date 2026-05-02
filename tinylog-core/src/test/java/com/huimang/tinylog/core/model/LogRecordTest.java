package com.huimang.tinylog.core.model;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

public class LogRecordTest {
    @Test
    void shouldExposeImmutableAttributes() {
        LogRecord record = new LogRecord(1L, LogLevel.INFO, "app", "main", "hello", null);

        assertEquals(LogLevel.INFO, record.getLevel());
        assertEquals("app", record.getSource());
        assertEquals("main", record.getContext());
        assertEquals("app", record.getLoggerName());
        assertEquals("main", record.getThreadName());
        assertTrue(record.getAttributes().isEmpty());
    }
}
