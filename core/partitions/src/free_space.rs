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

use server_common::DiskUsage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The free space a node keeps on its data disk, shared by every partition of
/// the node and by the sampler that measures the disk.
///
/// Reclaim writes before it frees, and a node whose WAL write fails stops. So
/// a primary refuses new messages while the disk is short, and everything
/// that frees space keeps running.
///
/// A sample alone is a second old, and a burst can write through the reserve
/// in less. So the free space is the last sample minus what the disk usage
/// estimate grew by since, which the write sites charge as they write.
#[derive(Debug)]
pub struct FreeSpaceReserve {
    reserve_bytes: u64,
    usage: &'static DiskUsage,
    sampled_free_bytes: AtomicU64,
    sampled_used_bytes: AtomicU64,
    /// Whether the last sample was short, for the sampler to log a crossing
    /// once. Admission reads the estimate instead.
    sampled_short: AtomicBool,
    refused_requests: AtomicU64,
}

/// What a sample changed, for the sampler to log once per crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeSpaceChange {
    Unchanged,
    FellShort,
    Recovered,
}

impl FreeSpaceReserve {
    /// A reserve of 0 never refuses. `usage` is the estimate of the volume
    /// that the samples describe.
    #[must_use]
    pub const fn new(reserve_bytes: u64, usage: &'static DiskUsage) -> Self {
        Self {
            reserve_bytes,
            usage,
            sampled_free_bytes: AtomicU64::new(u64::MAX),
            sampled_used_bytes: AtomicU64::new(0),
            sampled_short: AtomicBool::new(false),
            refused_requests: AtomicU64::new(0),
        }
    }

    /// Records a sample of the data disk: its free bytes, and the used bytes
    /// the same measurement gave the estimate as its base.
    pub fn observe(&self, free_bytes: u64, used_bytes: u64) -> FreeSpaceChange {
        // A reader between the two stores pairs the old free bytes with the
        // new used bytes. Growth since the sample then reads as zero for an
        // instant, which the next read corrects.
        self.sampled_used_bytes.store(used_bytes, Ordering::Relaxed);
        self.sampled_free_bytes.store(free_bytes, Ordering::Relaxed);
        let short = free_bytes < self.reserve_bytes;
        match (self.sampled_short.swap(short, Ordering::Relaxed), short) {
            (false, true) => FreeSpaceChange::FellShort,
            (true, false) => FreeSpaceChange::Recovered,
            _ => FreeSpaceChange::Unchanged,
        }
    }

    /// Whether the estimated free space is below the reserve.
    #[must_use]
    pub fn is_short(&self) -> bool {
        self.free_bytes() < self.reserve_bytes
    }

    /// Whether a primary must refuse new messages now. Counts the refusal.
    #[must_use]
    pub fn refuses_messages(&self) -> bool {
        let short = self.is_short();
        if short {
            self.refused_requests.fetch_add(1, Ordering::Relaxed);
        }
        short
    }

    #[must_use]
    pub const fn reserve_bytes(&self) -> u64 {
        self.reserve_bytes
    }

    /// The last sample minus what was written since, or `u64::MAX` before the
    /// first sample. Space freed since the sample shows with the next one.
    #[must_use]
    pub fn free_bytes(&self) -> u64 {
        let sampled_free = self.sampled_free_bytes.load(Ordering::Relaxed);
        let sampled_used = self.sampled_used_bytes.load(Ordering::Relaxed);
        let written_since = self
            .usage
            .used_bytes()
            .map_or(0, |used| used.saturating_sub(sampled_used));
        sampled_free.saturating_sub(written_since)
    }

    #[must_use]
    pub fn refused_requests(&self) -> u64 {
        self.refused_requests.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESERVE: u64 = 1024;
    const USED: u64 = 1 << 30;

    #[test]
    fn given_no_sample_when_asked_should_not_refuse() {
        static USAGE: DiskUsage = DiskUsage::new();
        let reserve = FreeSpaceReserve::new(RESERVE, &USAGE);

        assert!(!reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 0);
    }

    #[test]
    fn given_free_space_below_the_reserve_when_sampled_should_refuse_until_space_returns() {
        static USAGE: DiskUsage = DiskUsage::new();
        USAGE.rebase(|| Some(((), USED)));
        let reserve = FreeSpaceReserve::new(RESERVE, &USAGE);

        assert_eq!(reserve.observe(RESERVE, USED), FreeSpaceChange::Unchanged);
        assert!(!reserve.refuses_messages());
        assert_eq!(
            reserve.observe(RESERVE - 1, USED),
            FreeSpaceChange::FellShort
        );
        assert_eq!(reserve.observe(0, USED), FreeSpaceChange::Unchanged);
        assert!(reserve.refuses_messages());
        assert!(reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 2);
        assert_eq!(reserve.observe(RESERVE, USED), FreeSpaceChange::Recovered);
        assert!(!reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 2);
        assert_eq!(reserve.free_bytes(), RESERVE);
    }

    /// The sampler runs once a second, and a burst can write through the
    /// reserve in less.
    #[test]
    fn given_writes_since_the_sample_when_they_cross_the_reserve_should_refuse_before_the_next_sample()
     {
        static USAGE: DiskUsage = DiskUsage::new();
        USAGE.rebase(|| Some(((), USED)));
        let reserve = FreeSpaceReserve::new(RESERVE, &USAGE);
        reserve.observe(RESERVE + 100, USED);

        drop(USAGE.charge(100));
        assert_eq!(reserve.free_bytes(), RESERVE);
        assert!(!reserve.refuses_messages());
        let in_flight = USAGE.charge(1);
        assert!(
            reserve.refuses_messages(),
            "a write still in flight already takes its space"
        );
        drop(in_flight);

        // The next sample holds the writes, so they are not counted twice.
        USAGE.rebase(|| Some(((), USED + 101)));
        assert_eq!(
            reserve.observe(RESERVE - 1, USED + 101),
            FreeSpaceChange::FellShort
        );
        assert_eq!(reserve.free_bytes(), RESERVE - 1);

        // Deleted segments show with the sample that measures them.
        USAGE.rebase(|| Some(((), USED)));
        assert_eq!(
            reserve.observe(RESERVE + 100, USED),
            FreeSpaceChange::Recovered
        );
        assert!(!reserve.refuses_messages());
    }

    #[test]
    fn given_a_reserve_of_zero_when_the_disk_is_full_should_not_refuse() {
        static USAGE: DiskUsage = DiskUsage::new();
        USAGE.rebase(|| Some(((), USED)));
        let reserve = FreeSpaceReserve::new(0, &USAGE);

        assert_eq!(reserve.observe(0, USED), FreeSpaceChange::Unchanged);
        drop(USAGE.charge(RESERVE));
        assert!(!reserve.refuses_messages());
    }
}
