// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Seek, SeekFrom},
    time::SystemTime,
};

use uuid::{Builder, Timestamp, Uuid};

/// Returns the largest UUID v7 for the millisecond containing `time`.
///
/// UUID v7 stores a Unix timestamp with millisecond precision. This function
/// truncates any sub-millisecond portion of `time` and sets all remaining
/// variable bits to their maximum values, while preserving the required UUID
/// version and variant bits.
///
/// When compared in standard UUID byte order, every UUID v7 whose timestamp
/// falls in this millisecond or an earlier one sorts at or before the result.
/// This makes the result useful as an inclusive upper bound for queries over
/// UUID v7 identifiers.
///
/// # Precision
///
/// The cutoff includes the entire millisecond containing `time`, not just the
/// exact instant supplied. For example, a time of 1.234567890 seconds after the
/// Unix epoch produces a cutoff covering all of millisecond 1234, including
/// instants slightly later than the supplied time.
///
/// This ordering describes UUID timestamps, not necessarily actual commit
/// times: clock skew or the way identifiers are generated can affect that
/// relationship.
///
/// # Errors
///
/// Returns an [`std::io::Error`] with kind [`std::io::ErrorKind::InvalidInput`]
/// if `time` cannot be converted to a [`Timestamp`].
pub fn convert_systemtime_to_uuid(time: SystemTime) -> std::io::Result<Uuid> {
    let (secs, nanos) = Timestamp::try_from(time)
        .map_err(|_err| {
            Error::new(
                ErrorKind::InvalidInput,
                "Failed to convert SystemTime to Timestamp",
            )
        })?
        .to_unix();
    let millis = secs * 1000 + u64::from(nanos / 1_000_000);

    Ok(Builder::from_unix_timestamp_millis(millis, &[0xFF; 10]).into_uuid())
}

/// Reads the whole file from the start, then puts the cursor back where it was.
pub fn read_all(file: &mut File) -> std::io::Result<Vec<u8>> {
    let pos = file.stream_position()?;
    file.rewind()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    file.seek(SeekFrom::Start(pos))?;
    Ok(bytes)
}
