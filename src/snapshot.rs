// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{io::ErrorKind, sync::LazyLock};

use oxicode::{
    compression::{Compression, compress, decompress},
    config::Configuration,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

static ENCODING_CONFIG: LazyLock<Configuration> = LazyLock::new(oxicode::config::standard);

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotData {
    snapshots: Vec<Snapshot>,
}

impl SnapshotData {
    pub fn push(&mut self, snapshot: Snapshot) {
        self.snapshots.push(snapshot);
    }

    pub fn last(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    /// Decodes snapshot history; empty bytes (a fresh `.chrono` file) is empty history.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }

        // decompress() caps the regenerated size at 256 MiB by default (bomb
        // protection); use decompress_with_limit(data, max_output) to override it.
        // TODO: handle histories larger than the 256 MiB cap.
        let decompressed = decompress(bytes)
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err.to_string()))?;

        let (decoded, _size) =
            oxicode::serde::decode_from_slice(&decompressed, *ENCODING_CONFIG)
                .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
        Ok(decoded)
    }

    /// Returns the latest snapshot whose id is at or before `target`, or `None`
    /// if every snapshot is newer.
    ///
    /// Uses binary search for O(log n) lookups. Assumes `snapshots` is sorted by id.
    pub fn search(&self, target: &Uuid) -> Option<&Snapshot> {
        debug_assert!(self.snapshots.is_sorted_by_key(|s| s.id));

        // Stop early when empty or when the target is older than the first snapshot.
        let first = self.snapshots.first()?;
        if target < &first.id {
            return None;
        }

        let idx = self.snapshots.partition_point(|snap| target >= &snap.id);
        self.snapshots.get(idx - 1)
    }

    /// Encodes and compresses the snapshot history for the `.chrono` file.
    pub fn to_vec(&self) -> std::io::Result<Vec<u8>> {
        let encoded = oxicode::serde::encode_to_vec(&self, *ENCODING_CONFIG)
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err.to_string()))?;

        let compressed = compress(&encoded, Compression::Lz4)
            .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err.to_string()))?;

        Ok(compressed)
    }
}

impl IntoIterator for SnapshotData {
    type Item = Snapshot;
    type IntoIter = std::vec::IntoIter<Snapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.snapshots.into_iter()
    }
}

/// One committed version of the main file, as returned by
/// [`History::list_snapshots`](crate::History::list_snapshots).
///
/// Two snapshots compare equal when their contents' checksums match,
/// regardless of id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot UUID v7
    pub id: Uuid,
    /// crc32 checksum of the data
    checksum: u32,
    /// Uncompressed bytes of the snapshot
    data: Vec<u8>,
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.checksum == other.checksum
    }
}

impl Eq for Snapshot {}

impl Snapshot {
    /// Creates a snapshot of `data` with a fresh UUID v7 id and its crc32
    /// checksum.
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        let data = data.into();
        let checksum = crc32fast::hash(&data);
        Self {
            id: Uuid::now_v7(),
            checksum,
            data,
        }
    }

    /// Checks the stored data against its crc32 checksum.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidData`](std::io::ErrorKind::InvalidData) if the
    /// checksum does not match.
    pub fn validate(&self) -> std::io::Result<()> {
        if crc32fast::hash(&self.data) == self.checksum {
            Ok(())
        } else {
            Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Snapshot checksum's do not match. Data is corrupted.".to_string(),
            ))
        }
    }

    /// Returns a copy of the file contents stored in this snapshot.
    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.clone()
    }
}

#[cfg(test)]
mod tests {

    use uuid::{NoContext, Timestamp};

    use super::*;

    fn id_at(secs: u64, nanos: u32) -> Uuid {
        Uuid::new_v7(Timestamp::from_unix(NoContext, secs, nanos))
    }

    /// Snapshots with fixed, whole-second ids so ordering never depends on the clock.
    fn data_at(secs: &[u64]) -> SnapshotData {
        SnapshotData {
            snapshots: secs
                .iter()
                .map(|&s| Snapshot {
                    id: id_at(s, 0),
                    ..Snapshot::new(vec![])
                })
                .collect(),
        }
    }

    #[test]
    fn search_finds_latest_at_or_before_target() {
        let data = data_at(&[10, 20, 30]);
        let ids: Vec<Uuid> = data.snapshots.iter().map(|s| s.id).collect();

        // (target, expected id) — assert on id: Snapshot::eq only compares checksums.
        let cases = [
            (id_at(5, 0), None),
            (ids[0], Some(ids[0])),
            (ids[1], Some(ids[1])),
            (id_at(25, 500_000_000), Some(ids[1])),
            (ids[2], Some(ids[2])),
            (id_at(99, 0), Some(ids[2])),
        ];
        for (target, expected) in cases {
            assert_eq!(
                data.search(&target).map(|s| s.id),
                expected,
                "target {target}"
            );
        }
    }

    #[test]
    fn search_empty_returns_none() {
        assert!(SnapshotData::default().search(&id_at(10, 0)).is_none());
    }

    #[test]
    fn search_single_snapshot() {
        let data = data_at(&[10]);
        let id = data.snapshots[0].id;

        assert_eq!(data.search(&id_at(9, 0)).map(|s| s.id), None);
        assert_eq!(data.search(&id_at(11, 0)).map(|s| s.id), Some(id));
    }
}
