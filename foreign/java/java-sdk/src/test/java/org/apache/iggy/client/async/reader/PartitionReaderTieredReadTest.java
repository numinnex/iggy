/*
 * Licensed to the Apache Software Foundation (ASF) under one
 * or more contributor license agreements.  See the NOTICE file
 * distributed with this work for additional information
 * regarding copyright ownership.  The ASF licenses this file
 * to you under the Apache License, Version 2.0 (the
 * "License"); you may not use this file except in compliance
 * with the License.  You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.apache.iggy.client.async.reader;

import org.apache.iggy.client.async.reader.FakeTransport.Read;
import org.apache.iggy.exception.IggyErrorCode;
import org.apache.iggy.message.PollingStrategy;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.util.concurrent.TimeUnit;

import static org.apache.iggy.client.async.reader.FakeTransport.TIMEOUT;
import static org.apache.iggy.client.async.reader.FakeTransport.refused;
import static org.apache.iggy.client.async.reader.PartitionReaderTest.MANUAL;
import static org.apache.iggy.client.async.reader.PartitionReaderTest.QUIET;
import static org.apache.iggy.client.async.reader.PartitionReaderTest.next;
import static org.apache.iggy.client.async.reader.PartitionReaderTest.serverError;
import static org.apache.iggy.client.async.reader.PartitionReaderTest.standalone;
import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/** A read that the object store refuses for good stops its partition at once. */
class PartitionReaderTieredReadTest {

    /** The object store failed for good, so the first refusal stops the partition without a retry. */
    @ParameterizedTest
    @ValueSource(booleans = {false, true})
    void shouldStopAPartitionAtTheFirstTieredReadFailureUntilASeek(boolean wholeRequest) throws Exception {
        FakeTransport transport = new FakeTransport(false);
        PartitionReader reader = standalone(transport, MANUAL, 1L);
        try {
            Read refusedRead = transport.nextRead();
            if (wholeRequest) {
                refusedRead.reply().completeExceptionally(serverError(IggyErrorCode.TIERED_READ_FAILED));
            } else {
                refusedRead.answer(refused(1, IggyErrorCode.TIERED_READ_FAILED.getCode()));
            }

            assertThatThrownBy(() -> next(reader))
                    .hasCauseInstanceOf(ReadException.class)
                    .satisfies(error -> assertThat(((ReadException) error.getCause()).errorCode())
                            .hasValue(IggyErrorCode.TIERED_READ_FAILED.getCode()));
            assertThat(transport.readsNothingFor(QUIET)).isTrue();

            reader.seek(1L, PollingStrategy.first()).get(TIMEOUT.toMillis(), TimeUnit.MILLISECONDS);
            assertThat(transport.nextRead().targets()).containsKey(1L);
        } finally {
            reader.closeAsync().get(TIMEOUT.toMillis(), TimeUnit.MILLISECONDS);
        }
    }
}
