// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

#![deny(clippy::pedantic)]

mod chrono_file;
mod patches;
mod utils;
mod versions;

pub use chrono_file::ChronoFile;
pub use versions::VersionInfo;

use std::io;
use std::time::SystemTime;

/// Read access to a file's committed version history.
pub trait History {
    /// Lists every committed version, oldest first, with its id and commit
    /// time so a caller can decide which one to [`preview`](History::preview)
    /// or [`restore`](History::restore).
    ///
    /// Returns an empty vec when nothing has been committed yet.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the `.chrono` log cannot be read or decoded.
    fn list_versions(&mut self) -> io::Result<Vec<VersionInfo>>;

    /// Returns the file's contents as of `version` **without touching the main
    /// file** — a read-only peek to compare versions before restoring.
    ///
    /// `version` is a zero-based commit id (see
    /// [`list_versions`](History::list_versions)); the contents are
    /// reconstructed by replaying patches `0..=version`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the log cannot be read/replayed.
    fn preview(&mut self, version: u64) -> io::Result<Vec<u8>>;

    /// Like [`preview`](History::preview), but selects the version by time:
    /// the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the log cannot be
    /// read/replayed.
    fn preview_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>>;

    /// Restores the file to its contents as of `version` and returns those
    /// bytes.
    ///
    /// `version` is a zero-based commit id (as returned by
    /// [`commit`](ChronoFile::commit)); the contents are reconstructed by
    /// replaying patches `0..=version`. The main file is rewound, overwritten
    /// with the restored bytes, and truncated to their length, so its contents
    /// afterwards match exactly.
    ///
    /// The restore is itself **recorded as a new version** and becomes the new
    /// snapshot baseline, so a subsequent [`commit`](ChronoFile::commit) diffs
    /// against the restored contents (not the pre-restore ones). Restoring the
    /// latest version is a no-op that records nothing.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the log cannot be read/replayed or
    /// the main file cannot be rewritten.
    fn restore(&mut self, version: u64) -> io::Result<Vec<u8>>;

    /// Like [`restore`](History::restore), but selects the version by time:
    /// restores the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the log cannot be
    /// read/replayed or the main file cannot be rewritten.
    fn restore_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>>;
}
