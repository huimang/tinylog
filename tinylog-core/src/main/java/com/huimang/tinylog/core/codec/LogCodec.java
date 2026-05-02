package com.huimang.tinylog.core.codec;

import com.huimang.tinylog.core.model.LogRecord;
import java.io.IOException;

/**
 * Defines how a language-neutral log record is converted to and from the TinyLog binary format.
 */
public interface LogCodec {
    /**
     * Encodes one logical record for persistence or transfer.
     */
    byte[] encode(LogRecord record) throws IOException;

    /**
     * Decodes one previously stored record.
     */
    LogRecord decode(byte[] encoded) throws IOException;

    /**
     * Returns the stable codec name for negotiation and metadata.
     */
    String name();
}
