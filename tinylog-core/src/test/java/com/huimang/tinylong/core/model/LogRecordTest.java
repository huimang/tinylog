package com.huimang.tinylong.core.model;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

public class LogRecordTest {
    @Test
    void shouldExposeImmutableAttributes() {
        LogRecord record = new LogRecord(1L, LogLevel.INFO, "app", "main", "hello", null);

        assertEquals(LogLevel.INFO, record.getLevel());
        assertTrue(record.getAttributes().isEmpty());
    }
}

