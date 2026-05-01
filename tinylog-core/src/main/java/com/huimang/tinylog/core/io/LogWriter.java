package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.Closeable;
import java.io.IOException;

/**
 * Appends logical records to a tinylog destination.
 */
public interface LogWriter extends Closeable {
    /**
     * Writes one record to the underlying destination.
     */
    void append(LogRecord record) throws IOException;

    /**
     * Forces buffered state to be persisted.
     */
    void flush() throws IOException;
}
