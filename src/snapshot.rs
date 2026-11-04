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
    /// Uses binary search for O(log n) updates
    pub fn search(&self, target: &Uuid) -> Option<&Snapshot> {
        if self.snapshots.is_empty() {
            return None;
        }

        // Check target is > first snapshot entry. Drop any refs after check
        {
            if let Some(first) = self.snapshots.first() {
                if target < &first.id {
                    return None;
                }
            } else {
                return None;
            }
        }

        let mut l = 0 as usize;
        let mut r = self.snapshots.len() - 1;
        while r - l > 1 {
            let mid = l + (r - l) / 2;
            // SAFETY: we know mid is always a valid range therefore unwrap is fine
            let found = self.snapshots.get(mid)?;
            // TODO: review if comparison is quicker by converting SystemTime to Uuid or Uuid back
            // to system time is faster
            if &found.id < target {
                l = mid + 1;
            } else if &found.id > target {
                r = mid - 1;
            } else {
                return Some(found);
            }
        }

        return self.snapshots.get(l);
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

mod tests {

    use std::time::{Duration, SystemTime};

    use rand::{
        distr::{Alphanumeric, SampleString},
        rng,
    };
    use uuid::{ContextV7, Timestamp};

    use super::*;

    const MAX_ROW_LENGTH: usize = 64;
    const INITIAL_FILE_ROWS: usize = 24;

    fn generate_snapshots() -> SnapshotData {
        let mut data = SnapshotData::default();

        for i in 0..12 {
            data.push(generate_snapshot())
        }
        data
    }

    fn generate_snapshot() -> Snapshot {
        // artificial sleep for versioning
        std::thread::sleep(Duration::from_millis(100));

        let data = generate_snapshot_data();
        Snapshot::new(data)
    }

    fn generate_snapshot_data() -> Vec<u8> {
        let mut data = Vec::with_capacity(MAX_ROW_LENGTH);
        for i in 1..=MAX_ROW_LENGTH {
            data.push(generate_line())
        }
        data.join("\n").as_bytes().to_vec()
    }

    fn generate_line() -> String {
        Alphanumeric.sample_string(&mut rand::rng(), rand::random_range(1..=MAX_ROW_LENGTH))
    }

    #[test]
    fn test_search() {
        // create data select timestamp + a few micros
        let data = generate_snapshots();
        let len = data.snapshots.len();

        let expected = &data.snapshots[len / 2];
        let (seconds, nanos) = expected.id.get_timestamp().unwrap().to_unix();

        let context = ContextV7::new();
        let target = Uuid::new_v7(Timestamp::from_unix(&context, seconds, nanos + 50));

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
        std::thread::sleep(Duration::from_millis(500));

        let data = generate_snapshots();
        let output = data.search(&target);
        assert!(output.is_none());
    }

    #[test]
    fn test_search_target_older_than_latest() {
        let mut data = generate_snapshots();
        let len = data.snapshots.len();
        let latest = &data.snapshots[len - 1];

        let target = Uuid::now_v7();
        let output = data.search(&target);
        assert!(output.is_some());
        let output = output.unwrap().to_owned();
        assert_eq!(latest, &output);
    }
}
