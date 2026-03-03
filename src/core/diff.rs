use std::time::{SystemTime, UNIX_EPOCH};

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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_deser() {
        todo!()
    }
}
