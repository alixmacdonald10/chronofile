// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{
    fs::File,
    io::{BufReader, Read},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotData {
    snapshots: Vec<Snapshot>,
}

impl SnapshotData {
    fn push(&mut self, snapshot: Snapshot) {
        self.snapshots.push(snapshot)
    }

    /// Finds the snapshot at a given time denoted by a Uuid v7
    ///
    /// Uses binary search for O(log n) lookups. Assumes `snapshots` is sorted by id.
    pub fn search(&self, target: &Uuid) -> Option<&Snapshot> {
        debug_assert!(self.snapshots.is_sorted_by_key(|s| s.id));

        // Early stops on empty, or target older than first snapshot
        let first = self.snapshots.first()?;
        if target < &first.id {
            return None;
        }

        // Loop invariant:
        //   every snapshot before l has id <= target
        //   every snapshot from r onward has id > target
        let mut l = 0;
        let mut r = self.snapshots.len();
        while l < r {
            let mid = l + (r - l) / 2;
            let found = &self.snapshots[mid];
            if &found.id == target {
                return Some(found);
            } else if &found.id < target {
                l = mid + 1;
            } else {
                r = mid;
            }
        }
        self.snapshots.get(l - 1)
    }
}

impl IntoIterator for SnapshotData {
    type Item = Snapshot;
    type IntoIter = std::vec::IntoIter<Snapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.snapshots.into_iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot UUID v7
    id: Uuid,
    /// crc32 checksum of the data
    checksum: u32,
    /// Uncompressed bytes of the snapshot
    data: Vec<u8>,
}

impl Snapshot {
    pub fn new(data: Vec<u8>) -> Self {
        let checksum = crc32fast::hash(&data);
        Self {
            id: Uuid::now_v7(),
            checksum,
            data,
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.data.clone()
    }
}

impl TryFrom<File> for Snapshot {
    type Error = std::io::Error;

    fn try_from(value: File) -> Result<Self, Self::Error> {
        let id = Uuid::now_v7();

        let mut buf_reader = BufReader::new(value);
        let mut bytes: Vec<u8> = Vec::new();
        buf_reader.read_to_end(&mut bytes)?;

        let checksum = crc32fast::hash(&bytes);

        Ok(Self {
            id,
            checksum,
            data: bytes,
        })
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use arbitrary::{Arbitrary, Result, Unstructured};
    use uuid::{ContextV7, Timestamp};

    use super::*;

    const MAX_ROW_LENGTH: usize = 64;
    const INITIAL_FILE_ROWS: usize = 24;

    const MIN_SNAPSHOTS: usize = 3;
    const MAX_SNAPSHOTS: usize = 12;
    const MAX_DATA_LEN: usize = MAX_ROW_LENGTH * INITIAL_FILE_ROWS;

    impl<'a> Arbitrary<'a> for Snapshot {
        fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
            // spacing so v7 ids differ by >= 2ms (test_search offsets target by +1ms)
            std::thread::sleep(Duration::from_millis(2));

            let len = u.int_in_range(0..=MAX_DATA_LEN)?;
            let data = u.bytes(len)?.to_vec();
            Ok(Snapshot::new(data))
        }
    }

    impl<'a> Arbitrary<'a> for SnapshotData {
        fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
            let n = u.int_in_range(MIN_SNAPSHOTS..=MAX_SNAPSHOTS)?;
            let snapshots = (0..n)
                .map(|_| Snapshot::arbitrary(u))
                .collect::<Result<Vec<_>>>()?;
            Ok(Self { snapshots })
        }
    }

    fn generate_snapshots() -> SnapshotData {
        // Sized for worst case: MAX_SNAPSHOTS snapshots, each up to MAX_DATA_LEN bytes
        // of payload + a few bytes consumed by `int_in_range` per call. Too small and
        // `u.bytes(len)` fails with `NotEnoughData`.
        let mut raw = vec![0u8; MAX_SNAPSHOTS * (MAX_DATA_LEN + 8) + 8];
        rand::fill(&mut raw[..]);
        let mut u = Unstructured::new(&raw);
        SnapshotData::arbitrary(&mut u).expect("arbitrary SnapshotData")
    }

    #[test]
    fn test_search() {
        // create data, select timestamp + 1ms. v7 ids only have ms precision, so a
        // smaller offset lands in the same ms with random low bits that may sort
        // before `expected`.
        let data = generate_snapshots();
        let len = data.snapshots.len();

        let expected = &data.snapshots[len / 2];
        let (seconds, nanos) = expected.id.get_timestamp().unwrap().to_unix();

        let context = ContextV7::new();
        let target = Uuid::new_v7(Timestamp::from_unix(&context, seconds, nanos + 1_000_000));

        let output = data.search(&target);
        assert!(output.is_some());
        let output = output.unwrap().to_owned();
        assert_eq!(expected, &output);
    }

    #[test]
    fn test_search_target_is_identical_match() {
        // create data select timestamp + a few micros
        let data = generate_snapshots();
        let len = data.snapshots.len();

        let expected = &data.snapshots[len / 2];
        let target = expected.id;

        let output = data.search(&target);
        assert!(output.is_some());
        let output = output.unwrap().to_owned();
        assert_eq!(expected, &output);
    }

    #[test]
    fn test_search_target_less_than_range() {
        let target = Uuid::now_v7();
        std::thread::sleep(Duration::from_millis(2));

        let data = generate_snapshots();
        let output = data.search(&target);
        assert!(output.is_none());
    }

    #[test]
    fn test_search_target_older_than_latest() {
        let data = generate_snapshots();
        let len = data.snapshots.len();
        let latest = &data.snapshots[len - 1];

        let target = Uuid::now_v7();
        let output = data.search(&target);
        assert!(output.is_some());
        let output = output.unwrap().to_owned();
        assert_eq!(latest, &output);
    }
}
