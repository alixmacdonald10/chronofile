// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

/// Current wall-clock time as milliseconds since the Unix epoch. Clocks set
/// before 1970 clamp to 0 rather than erroring — a version timestamp is
/// advisory metadata, never load-bearing.
pub(crate) fn now_ms() -> u64 {
    to_ms(std::time::SystemTime::now())
}

/// A [`SystemTime`](std::time::SystemTime) as milliseconds since the Unix
/// epoch. Times before 1970 clamp to 0.
pub(crate) fn to_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
