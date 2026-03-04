use std::{cmp::Ordering, fmt::{Debug, Display}, time::{Duration, SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Diff {
    pub timestamp: u64,
    uncompressed_hash: [u8; 32],
    compressed_hash: [u8; 32],
    pub compressed_data: Vec<u8>,
}

impl Diff {
    pub fn new(
        uncompressed_hash: [u8; 32],
        compressed_hash: [u8; 32],
        compressed_data: Vec<u8>,
    ) -> Self {
        // SAFETY: panics if System Time is < 1970
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed")
            .as_secs();

        Self {
            timestamp,
            uncompressed_hash,
            compressed_hash,
            compressed_data,
        }
    }
}

impl PartialEq for Diff {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }
}

impl Eq for Diff {}

impl PartialOrd for Diff {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Diff {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

impl From<Diff> for SystemTime {
    fn from(val: Diff) -> Self {
        UNIX_EPOCH + Duration::new(val.timestamp, 0)
    }
}

impl From<&Diff> for SystemTime {
    fn from(val: &Diff) -> Self {
        UNIX_EPOCH + Duration::new(val.timestamp, 0)
    }
}

impl Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.timestamp)
    }
}

impl Debug for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.timestamp)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

}
