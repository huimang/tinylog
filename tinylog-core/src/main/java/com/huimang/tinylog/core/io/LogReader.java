package com.huimang.tinylog.core.io;

import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import java.io.Closeable;
import java.io.IOException;
import java.util.Iterator;

/**
 * Streams log records from a TinyLog source without exposing language-specific runtime concepts.
 */
public interface LogReader extends Closeable {
    /**
     * Scans all available records in natural storage order.
     */
    Iterator<LogRecord> scan() throws IOException;

    /**
     * Scans records that satisfy the provided business query.
     */
    Iterator<LogRecord> scan(LogQuery query) throws IOException;
}
