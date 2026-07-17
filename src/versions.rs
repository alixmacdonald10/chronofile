// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

/// Metadata for one committed version, as returned by
/// [`list_versions`](History::list_versions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionInfo {
    /// Zero-based version id — pass this to [`restore`](History::restore) or
    /// [`preview`](History::preview).
    pub id: u64,
    /// Wall-clock time the version was committed.
    pub timestamp: std::time::SystemTime,
}
