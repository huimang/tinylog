package com.huimang.tinylog.sdk;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

public class NoopTinyLoggerFactoryTest {
    @Test
    void shouldCreateLoggerByClassName() {
        TinyLogger logger = new NoopTinyLoggerFactory().getLogger(NoopTinyLoggerFactoryTest.class);

        assertEquals(NoopTinyLoggerFactoryTest.class.getName(), logger.getName());
    }
}
