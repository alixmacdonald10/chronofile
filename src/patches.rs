// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

//! The versioned patch log stored in the companion `.chrono` file.

/// Wraps a decode/encode/patch error from a dependency in an
/// [`io::Error`](std::io::Error) so the public API only ever yields I/O errors,
/// never panics — matching [`std::fs::File`], which never panics on a bad read.
pub(crate) fn invalid_data<E: std::fmt::Display>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
}

/// One recorded version: the diff that produces it plus when it was committed.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Clone)]
pub(crate) struct Entry {
    /// Wall-clock commit time, milliseconds since the Unix epoch.
    pub(crate) timestamp_ms: u64,
    /// The [`diffy`] patch (its byte serialization) from the previous version
    /// to this one.
    pub(crate) patch: Vec<u8>,
}

/// An ordered log of per-version diffs.
///
/// Each entry is one [`diffy`] patch (its byte serialization) describing the
/// change from the previous version to the next, tagged with its commit time.
/// Entry `i` is version `i`; replaying entries `0..=i` reconstructs version
/// `i`'s full contents.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default)]
pub(crate) struct Patches(pub(crate) Vec<Entry>);

impl Patches {
    /// Decodes a patch log from its on-disk encoding.
    ///
    /// An empty buffer (a freshly created `.chrono` file) decodes to an empty
    /// log; a non-empty but corrupt buffer yields an
    /// [`InvalidData`](std::io::ErrorKind::InvalidData) error rather than
    /// panicking.
    pub(crate) fn decode(bytes: &[u8]) -> std::io::Result<Self> {
        if bytes.is_empty() {
            Ok(Self::default())
        } else {
            bincode2::deserialize(bytes).map_err(invalid_data)
        }
    }

    /// Encodes the log to its compact on-disk form.
    pub(crate) fn encode(&self) -> std::io::Result<Vec<u8>> {
        bincode2::serialize(self).map_err(invalid_data)
    }

    /// Appends a patch (its `diffy` byte serialization) as the newest version,
    /// stamped with `timestamp_ms` (milliseconds since the Unix epoch).
    pub(crate) fn push(&mut self, patch: Vec<u8>, timestamp_ms: u64) {
        self.0.push(Entry { timestamp_ms, patch });
    }

    /// Number of recorded versions.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Reconstructs the latest contents by replaying every patch in order onto
    /// an empty buffer.
    pub(crate) fn replay(&self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        for entry in &self.0 {
            let patch = diffy::Patch::from_bytes(&entry.patch).map_err(invalid_data)?;
            data = diffy::apply_bytes(&data, &patch).map_err(invalid_data)?;
        }
        Ok(data)
    }

    /// Returns the patch entries needed to reconstruct the version picked by
    /// `select`: entries `0..=i`, i.e. **up to and including** the resolved
    /// version `i`.
    ///
    /// Returns `None` when the selector resolves to no version — a
    /// [`Version`](Select::Version) id that is out of range, or an
    /// [`AsOf`](Select::AsOf) time earlier than the first commit.
    pub(crate) fn filter(&self, select: Select) -> Option<&[Entry]> {
        let idx = match select {
            Select::Version(v) => v,
            // latest version committed at or before `ts`
            Select::AsOf(ts) => self.0.iter().rposition(|e| e.timestamp_ms <= ts)?,
        };
        self.0.get(..=idx)
    }
}

/// Picks which recorded version [`Patches::filter`] resolves to.
pub(crate) enum Select {
    /// A zero-based version id.
    Version(usize),
    /// The latest version committed at or before this time (milliseconds since
    /// the Unix epoch).
    AsOf(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a log of `n` versions, each a diff from the previous full
    /// contents to `"version {i}\n"` repeated `0..=i` (so replaying `0..=i`
    /// yields a predictable, growing buffer).
    fn line(i: usize) -> Vec<u8> {
        format!("version {i}\n").into_bytes()
    }

    fn cumulative(i: usize) -> Vec<u8> {
        (0..=i).flat_map(line).collect()
    }

    fn build_log(n: usize) -> Patches {
        let mut patches = Patches::default();
        let mut prev = Vec::new();
        for i in 0..n {
            let next = cumulative(i);
            patches.push(diffy::create_patch_bytes(&prev, &next).to_bytes(), i as u64);
            prev = next;
        }
        patches
    }

    #[test]
    fn decode_empty_is_default() {
        assert_eq!(Patches::decode(&[]).unwrap(), Patches::default());
    }

    #[test]
    fn encode_decode_roundtrip() {
        let mut patches = Patches::default();
        patches.push(b"one".to_vec(), 100);
        patches.push(b"two".to_vec(), 200);

        let encoded = patches.encode().unwrap();
        assert_eq!(Patches::decode(&encoded).unwrap(), patches);
    }

    #[test]
    fn decode_corrupt_is_invalid_data() {
        let err = Patches::decode(&[0xff, 0xff, 0xff, 0xff]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn push_and_len() {
        let mut patches = Patches::default();
        assert_eq!(patches.len(), 0);
        patches.push(b"p".to_vec(), 1);
        patches.push(b"q".to_vec(), 2);
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn filter_includes_named_version() {
        let patches = build_log(4);
        // version 2 => entries 0,1,2
        let entries = patches.filter(Select::Version(2)).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries, &patches.0[..3]);
    }

    #[test]
    fn filter_version_zero_returns_first_entry() {
        let patches = build_log(3);
        assert_eq!(patches.filter(Select::Version(0)).unwrap().len(), 1);
    }

    #[test]
    fn filter_out_of_range_is_none() {
        let patches = build_log(3);
        assert!(patches.filter(Select::Version(3)).is_none());
        assert!(patches.filter(Select::Version(99)).is_none());
    }

    #[test]
    fn filter_as_of_picks_latest_at_or_before() {
        // build_log stamps entry i with timestamp_ms = i
        let patches = build_log(5); // timestamps 0,1,2,3,4
        // exactly on a commit time => that version
        assert_eq!(patches.filter(Select::AsOf(2)).unwrap().len(), 3);
        // between commits => the earlier version
        assert_eq!(patches.filter(Select::AsOf(3)).unwrap().len(), 4);
        // after the last commit => the latest version
        assert_eq!(patches.filter(Select::AsOf(999)).unwrap().len(), 5);
    }

    #[test]
    fn filter_as_of_before_first_commit_is_none() {
        let patches = build_log(3); // earliest timestamp is 0
        assert!(patches.filter(Select::AsOf(0)).unwrap().len() == 1);
        // build_log's first timestamp is 0, so nothing is strictly before it;
        // use a log that starts later to exercise the None path
        let mut later = Patches::default();
        later.push(b"p".to_vec(), 50);
        assert!(later.filter(Select::AsOf(49)).is_none());
    }

    #[test]
    fn replay_reconstructs_full_contents() {
        let patches = build_log(5);
        assert_eq!(patches.replay().unwrap(), cumulative(4));
    }

    #[test]
    fn replay_empty_log_is_empty() {
        assert_eq!(Patches::default().replay().unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn filtered_replay_matches_that_version() {
        let patches = build_log(5);
        for v in 0..5 {
            let entries = patches.filter(Select::Version(v)).unwrap();
            let data = Patches(entries.to_vec()).replay().unwrap();
            assert_eq!(data, cumulative(v), "version {v} mismatch");
        }
    }
}
