// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

#![deny(clippy::pedantic)]

mod chrono_file;
mod snapshot;
mod utils;

pub use chrono_file::ChronoFile;

use uuid::Uuid;

use std::io;
use std::time::SystemTime;

use crate::snapshot::Snapshot;

/// Read access to a file's committed version history.
pub trait History {
    /// Lists every snapshot, oldest first, with its id and commit
    /// time so a caller can decide which one to [`preview`](History::preview)
    /// or [`restore`](History::restore).
    ///
    /// Returns an empty vec when nothing has been committed yet.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the `.chrono` file cannot be read or decoded.
    fn list_snapshots(&mut self) -> io::Result<Vec<Snapshot>>;

    /// Returns the file's contents as of `version` **without touching the main
    /// file** — a read-only peek to compare versions before restoring.
    ///
    /// `version` is a zero-based commit id (see
    /// [`list_versions`](History::list_versions)); the contents are read from
    /// the stored snapshot for that version.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the `.chrono` file cannot be
    /// read/decoded.
    fn preview(&mut self, version: Uuid) -> io::Result<Option<Vec<u8>>>;

    /// Like [`preview`](History::preview), but selects the version by time:
    /// the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the `.chrono` file
    /// cannot be read/decoded.
    fn preview_at(&mut self, time: SystemTime) -> io::Result<Option<Vec<u8>>>;

    /// Restores the file to its contents as of `version` and returns those
    /// bytes.
    ///
    /// `version` is a zero-based commit id (as returned by
    /// [`commit`](ChronoFile::commit)). The main file is rewound, overwritten
    /// with the restored bytes, and truncated to their length, so its contents
    /// afterwards match exactly.
    ///
    /// The restore is itself **recorded as a new version**, so the restored
    /// contents become the latest version. Restoring the latest version is a
    /// no-op that records nothing.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the `.chrono` file cannot be
    /// read/decoded or the main file cannot be rewritten.
    fn restore(&mut self, version: Uuid) -> io::Result<Vec<u8>>;

    /// Like [`restore`](History::restore), but selects the version by time:
    /// restores the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the `.chrono` file
    /// cannot be read/decoded or the main file cannot be rewritten.
    fn restore_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>>;

    /// Records the current contents of the main file as a new version and
    /// returns its version id (a Uuid V7).
    ///
    /// This is the **only** way a version is created. Writing to the file
    /// changes its bytes but records nothing until you commit. Each version is
    /// stored as a full snapshot in the `.chrono` file.
    ///
    /// If the file is byte-for-byte identical to the last commit, no version is
    /// created and `Ok(None)` is returned. Otherwise `Ok(Some(id))`.
    ///
    /// # Errors
    ///
    /// Returns an error if the main file cannot be read or the `.chrono` file
    /// cannot be written.
    fn commit(&mut self) -> io::Result<Option<Uuid>>;
}
