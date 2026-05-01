package com.huimang.tinylong.core.io;

import com.huimang.tinylong.core.model.LogQuery;
import com.huimang.tinylong.core.model.LogRecord;
import java.io.Closeable;
import java.io.IOException;
import java.util.Iterator;

/**
 * Streams log records from a tinylog source without forcing full-file loading.
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
