// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use super::*;
use crate::FreeSpaceReserve;
use iggy_binary_protocol::{
    PrepareHeader, RoutedRequestHeader, requests::partitions::TransitionPartitionHistoryRequest,
};
use iggy_common::IggyError;
use server_common::Message;
use std::sync::Arc;

const RESERVE_BYTES: u64 = 1 << 30;

fn short_reserve() -> Arc<FreeSpaceReserve> {
    let reserve = Arc::new(FreeSpaceReserve::new(RESERVE_BYTES));
    reserve.observe(RESERVE_BYTES - 1);
    reserve
}

fn send_request<B: MessageBus>(partition: &IggyPartition<B>) -> Message<RoutedRequestHeader> {
    let namespace = partition.namespace();
    let mut send = checksumless_send_request(namespace, 1).transmute_header(
        |header, next: &mut RoutedRequestHeader| {
            *next = header;
            next.partition_incarnation = partition.created_revision();
        },
    );
    send.as_mut_slice()[size_of::<RoutedRequestHeader>()..]
        .copy_from_slice(&build_segment_record(namespace, 0));
    send
}

#[compio::test]
async fn given_a_short_data_disk_when_the_primary_gets_messages_should_refuse_them_until_space_returns()
 {
    let (mut partition, _) = recording_partition_at(0, 3);
    partition.runtime_options.durability = iggy_common::Durability::Replicated;
    let reserve = short_reserve();
    partition.set_free_space_reserve(Some(Arc::clone(&reserve)));

    let (sender, refusal) = consensus::oneshot_channel();
    partition
        .on_request(send_request(&partition), Some(sender))
        .await;
    // Checked before the await: a wrongly admitted send never gets a reply.
    assert_eq!(partition.consensus.sequencer().current_sequence(), 0);
    assert_eq!(
        refusal.await.unwrap().header().status,
        IggyError::TransientNotAccepted.as_code()
    );
    assert_eq!(reserve.refused_requests(), 1);

    reserve.observe(RESERVE_BYTES);
    partition.on_request(send_request(&partition), None).await;
    assert_eq!(partition.consensus.sequencer().current_sequence(), 1);
    assert_eq!(reserve.refused_requests(), 1);
}

/// Deleting the history of a partition is how its space comes back, so the
/// reserve must not refuse it.
#[compio::test]
async fn given_a_short_data_disk_when_the_primary_gets_a_history_deletion_should_admit_it() {
    let (mut partition, _) = recording_partition_at(0, 3);
    let reserve = short_reserve();
    partition.set_free_space_reserve(Some(Arc::clone(&reserve)));
    let transition = TransitionPartitionHistoryRequest {
        incarnation: partition.created_revision(),
        metadata_op: 10,
    };

    partition
        .on_request(
            partition_control_request(
                &partition,
                Operation::TransitionPartitionHistory,
                &transition.to_bytes(),
                transition.metadata_op,
            ),
            None,
        )
        .await;

    assert_eq!(partition.consensus.sequencer().current_sequence(), 1);
    assert_eq!(reserve.refused_requests(), 0);
}

/// Only the node that is short refuses. A backup that is short still names
/// the primary to a client and still journals what the primary prepares, so
/// the group commits as before.
#[compio::test]
async fn given_a_short_data_disk_on_a_backup_only_when_the_primary_gets_messages_should_replicate_them()
 {
    let (mut primary, _) = recording_partition_at(0, 3);
    primary.runtime_options.durability = iggy_common::Durability::Replicated;
    primary.set_free_space_reserve(Some(Arc::new(FreeSpaceReserve::new(RESERVE_BYTES))));
    let (mut backup, _) = recording_partition_at(1, 3);
    backup.runtime_options.durability = iggy_common::Durability::Replicated;
    let reserve = short_reserve();
    backup.set_free_space_reserve(Some(Arc::clone(&reserve)));

    let (sender, refusal) = consensus::oneshot_channel();
    backup.on_request(send_request(&backup), Some(sender)).await;
    assert_eq!(
        refusal.await.unwrap().header().status,
        IggyError::NotPrimary.as_code()
    );

    primary.on_request(send_request(&primary), None).await;
    assert_eq!(primary.consensus.sequencer().current_sequence(), 1);
    let bytes = primary.log.journal().inner.repair_entry(1).unwrap();
    let prepare =
        Message::<PrepareHeader>::try_from(Owned::copy_from_slice(bytes.as_slice())).unwrap();
    backup.on_replicate(prepare).await;

    assert!(backup.log.journal().inner.holds_op(1));
    assert_eq!(reserve.refused_requests(), 0);
}
