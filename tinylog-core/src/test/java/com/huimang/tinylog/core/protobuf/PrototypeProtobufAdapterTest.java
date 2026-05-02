package com.huimang.tinylog.core.protobuf;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.huimang.tinylog.core.model.LogLevel;
import com.huimang.tinylog.core.model.LogQuery;
import com.huimang.tinylog.core.model.LogRecord;
import com.huimang.tinylog.proto.PrototypeLogQuery;
import com.huimang.tinylog.proto.PrototypeLogRecord;
import java.util.Collections;
import org.junit.jupiter.api.Test;

class PrototypeProtobufAdapterTest {
    @Test
    void shouldRoundTripLogRecordThroughProtobuf() {
        LogRecord record = new LogRecord(
                1_777_672_860_253L,
                LogLevel.WARN,
                "order-service",
                "queue-monitor",
                "queue depth is rising",
                Collections.singletonMap("tenant", "blue"));

        PrototypeLogRecord protobufRecord = PrototypeProtobufAdapter.toProtoRecord(record);
        LogRecord rebuiltRecord = PrototypeProtobufAdapter.toDomainRecord(protobufRecord);

        assertEquals(record.getTimestampMillis(), rebuiltRecord.getTimestampMillis());
        assertEquals(record.getLevel(), rebuiltRecord.getLevel());
        assertEquals(record.getSource(), rebuiltRecord.getSource());
        assertEquals(record.getContext(), rebuiltRecord.getContext());
        assertEquals(record.getMessage(), rebuiltRecord.getMessage());
        assertEquals(record.getAttributes(), rebuiltRecord.getAttributes());
    }

    @Test
    void shouldRoundTripLogQueryThroughProtobuf() {
        LogQuery query = LogQuery.builder()
                .startTimestampMillis(Long.valueOf(100L))
                .endTimestampMillis(Long.valueOf(200L))
                .minimumLevel(LogLevel.ERROR)
                .keyword("payment")
                .build();

        PrototypeLogQuery protobufQuery = PrototypeProtobufAdapter.toProtoQuery(query);
        LogQuery rebuiltQuery = PrototypeProtobufAdapter.toDomainQuery(protobufQuery);

        assertEquals(query.getStartTimestampMillis(), rebuiltQuery.getStartTimestampMillis());
        assertEquals(query.getEndTimestampMillis(), rebuiltQuery.getEndTimestampMillis());
        assertEquals(query.getMinimumLevel(), rebuiltQuery.getMinimumLevel());
        assertEquals(query.getKeyword(), rebuiltQuery.getKeyword());
    }
}
