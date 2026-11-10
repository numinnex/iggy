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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The free space a node keeps on its data disk, shared by every partition of
/// the node and by the sampler that measures the disk.
///
/// Reclaim writes before it frees, and a node whose WAL write fails stops. So
/// a primary refuses new messages while the disk is short, and everything
/// that frees space keeps running.
#[derive(Debug)]
pub struct FreeSpaceReserve {
    reserve_bytes: u64,
    free_bytes: AtomicU64,
    short: AtomicBool,
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
    /// A reserve of 0 never refuses.
    #[must_use]
    pub const fn new(reserve_bytes: u64) -> Self {
        Self {
            reserve_bytes,
            free_bytes: AtomicU64::new(u64::MAX),
            short: AtomicBool::new(false),
            refused_requests: AtomicU64::new(0),
        }
    }

    /// Records a sample of the free space on the data disk.
    pub fn observe(&self, free_bytes: u64) -> FreeSpaceChange {
        self.free_bytes.store(free_bytes, Ordering::Relaxed);
        let short = free_bytes < self.reserve_bytes;
        match (self.short.swap(short, Ordering::Relaxed), short) {
            (false, true) => FreeSpaceChange::FellShort,
            (true, false) => FreeSpaceChange::Recovered,
            _ => FreeSpaceChange::Unchanged,
        }
    }

    /// Whether the last sample was below the reserve.
    #[must_use]
    pub fn is_short(&self) -> bool {
        self.short.load(Ordering::Relaxed)
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

    /// The last sample, or `u64::MAX` before the first one.
    #[must_use]
    pub fn free_bytes(&self) -> u64 {
        self.free_bytes.load(Ordering::Relaxed)
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

    #[test]
    fn given_no_sample_when_asked_should_not_refuse() {
        let reserve = FreeSpaceReserve::new(RESERVE);

        assert!(!reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 0);
    }

    #[test]
    fn given_free_space_below_the_reserve_when_sampled_should_refuse_until_space_returns() {
        let reserve = FreeSpaceReserve::new(RESERVE);

        assert_eq!(reserve.observe(RESERVE), FreeSpaceChange::Unchanged);
        assert!(!reserve.refuses_messages());
        assert_eq!(reserve.observe(RESERVE - 1), FreeSpaceChange::FellShort);
        assert_eq!(reserve.observe(0), FreeSpaceChange::Unchanged);
        assert!(reserve.refuses_messages());
        assert!(reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 2);
        assert_eq!(reserve.observe(RESERVE), FreeSpaceChange::Recovered);
        assert!(!reserve.refuses_messages());
        assert_eq!(reserve.refused_requests(), 2);
        assert_eq!(reserve.free_bytes(), RESERVE);
    }

    #[test]
    fn given_a_reserve_of_zero_when_the_disk_is_full_should_not_refuse() {
        let reserve = FreeSpaceReserve::new(0);

        assert_eq!(reserve.observe(0), FreeSpaceChange::Unchanged);
        assert!(!reserve.refuses_messages());
    }
}
