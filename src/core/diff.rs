use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Diff {
    timestamp: u64,
    uncompressed_hash: [u8; 32],
    compressed_hash: [u8; 32],
    compressed_data: Vec<u8>,
}

impl Diff {
    pub fn new(
        uncompressed_hash: [u8; 32],
        compressed_hash: [u8; 32],
        compressed_data: Vec<u8>,
    ) -> Self {
        let datetime: DateTime<Utc> = SystemTime::now().into();
        // SAFETY: u64 will never fail as system time always > 1970
        let timestamp = datetime.timestamp().try_into().unwrap();

        Self {
            timestamp,
            uncompressed_hash,
            compressed_hash,
            compressed_data,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_deser() {
        todo!()
    }
}
