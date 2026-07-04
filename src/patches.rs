// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

//! The versioned patch log stored in the companion `.chrono` file.

/// Wraps a decode/encode/patch error from a dependency in an
/// [`io::Error`](std::io::Error) so the public API only ever yields I/O errors,
/// never panics — matching [`std::fs::File`], which never panics on a bad read.
pub(crate) fn invalid_data<E: std::fmt::Display>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
}

/// An ordered log of per-version diffs.
///
/// Each entry is one [`diffy`] patch (its byte serialization) describing the
/// change from the previous version to the next. Entry `i` is version `i`;
/// replaying entries `0..=i` reconstructs version `i`'s full contents.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default)]
pub(crate) struct Patches(pub(crate) Vec<Vec<u8>>);

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

    /// Appends a patch (its `diffy` byte serialization) as the newest version.
    pub(crate) fn push(&mut self, patch: Vec<u8>) {
        self.0.push(patch);
    }

    /// Number of recorded versions.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Reconstructs the latest contents by replaying every patch in order onto
    /// an empty buffer.
    pub(crate) fn replay(&self) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        for patch_bytes in &self.0 {
            let patch = diffy::Patch::from_bytes(patch_bytes).map_err(invalid_data)?;
            data = diffy::apply_bytes(&data, &patch).map_err(invalid_data)?;
        }
        Ok(data)
    }

    /// Returns the patch entries needed to reconstruct `version`: entries
    /// `0..=version`, i.e. **up to and including** `version`.
    ///
    /// Returns `None` if `version` is out of range (no such version exists),
    /// mirroring [`slice::get`].
    pub(crate) fn filter(&self, version: usize) -> Option<&[Vec<u8>]> {
        self.0.get(..=version)
    }
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
            patches.push(diffy::create_patch_bytes(&prev, &next).to_bytes());
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
        patches.push(b"one".to_vec());
        patches.push(b"two".to_vec());

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
        patches.push(b"p".to_vec());
        patches.push(b"q".to_vec());
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn filter_includes_named_version() {
        let patches = build_log(4);
        // version 2 => entries 0,1,2
        let entries = patches.filter(2).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries, &patches.0[..3]);
    }

    #[test]
    fn filter_version_zero_returns_first_entry() {
        let patches = build_log(3);
        assert_eq!(patches.filter(0).unwrap().len(), 1);
    }

    #[test]
    fn filter_out_of_range_is_none() {
        let patches = build_log(3);
        assert!(patches.filter(3).is_none());
        assert!(patches.filter(99).is_none());
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
            let entries = patches.filter(v).unwrap();
            let data = Patches(entries.to_vec()).replay().unwrap();
            assert_eq!(data, cumulative(v), "version {v} mismatch");
        }
    }
}
