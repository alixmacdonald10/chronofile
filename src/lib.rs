// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

#![deny(clippy::pedantic)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod chrono_file;
mod snapshot;
mod utils;

pub use chrono_file::ChronoFile;
pub use snapshot::Snapshot;

use uuid::Uuid;

use std::io;
use std::time::SystemTime;

/// Read access to a file's committed version history.
pub trait History {
    /// Lists every snapshot, oldest first, so a caller can pick one to
    /// [`preview`](History::preview) or [`restore`](History::restore). Each
    /// snapshot's `id` is a UUID v7, which encodes its commit time.
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
    /// `version` is a snapshot id (see
    /// [`list_snapshots`](History::list_snapshots)). The latest snapshot with
    /// an id **at or before** `version` is returned, or `None` if there is none.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidData`](std::io::ErrorKind::InvalidData) if the snapshot
    /// fails its checksum, or an I/O error if the `.chrono` file cannot be read
    /// or decoded.
    fn preview(&mut self, version: Uuid) -> io::Result<Option<Vec<u8>>>;

    /// Like [`preview`](History::preview), but selects the version by time:
    /// the latest snapshot committed **at or before** `time`, or `None` if
    /// there is none.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `time` is
    /// before the Unix epoch, [`InvalidData`](std::io::ErrorKind::InvalidData)
    /// if the snapshot fails its checksum, or an I/O error if the `.chrono` file
    /// cannot be read or decoded.
    fn preview_at(&mut self, time: SystemTime) -> io::Result<Option<Vec<u8>>>;

    /// Restores the file to its contents as of `version` and returns those
    /// bytes.
    ///
    /// `version` is a snapshot id (as returned by [`commit`](History::commit));
    /// like [`preview`](History::preview), the latest snapshot at or before it
    /// is used. The main file is rewound, overwritten with the restored bytes,
    /// and truncated to their length, so its contents afterwards match exactly.
    ///
    /// The restore is itself **recorded as a new version**, so the restored
    /// contents become the latest version. Restoring the latest version is a
    /// no-op that records nothing.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no snapshot
    /// exists at or before `version`,
    /// [`InvalidData`](std::io::ErrorKind::InvalidData) if the snapshot fails
    /// its checksum (the main file is left untouched), or an I/O error if the
    /// `.chrono` file cannot be read or decoded or the main file cannot be
    /// rewritten.
    fn restore(&mut self, version: Uuid) -> io::Result<Vec<u8>>;

    /// Like [`restore`](History::restore), but selects the version by time:
    /// restores the latest snapshot committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `time` is
    /// before the Unix epoch or no snapshot was committed at or before it,
    /// [`InvalidData`](std::io::ErrorKind::InvalidData) if the snapshot fails
    /// its checksum, or an I/O error if the `.chrono` file cannot be read or
    /// decoded or the main file cannot be rewritten.
    fn restore_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>>;

    /// Records the current contents of the main file as a new version and
    /// returns its version id (a UUID v7).
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
