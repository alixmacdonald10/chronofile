use std::io::{self, BufReader, BufWriter, Read, Write};

use snap::{read::FrameDecoder, write::FrameEncoder};

// TODO: error handle
// TODO: doc comments
pub fn compress(buf: &[u8]) -> io::Result<(Vec<u8>, usize)> {
    // compress file. stream read the file contents and compress to a buffer
    let buf_writer = BufWriter::new(Vec::new());
    let mut writer = FrameEncoder::new(buf_writer);
    let mut reader = BufReader::new(buf);
    io::copy(&mut reader, &mut writer)?;
    let compressed_data = writer
        .into_inner()
        .expect("failed to get buf reader")
        .into_inner()
        .expect("failed to get vec");
    let compressed_len = compressed_data.len();
    Ok((compressed_data, compressed_len))
}

// TODO: Doc comment
pub fn decompress(buf: &[u8]) -> io::Result<Vec<u8>> {
    let mut decoder = FrameDecoder::new(buf);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::TempDir;

    use super::*;

    pub fn create_temp_dir(prefix: &str) -> TempDir {
        TempDir::with_prefix(prefix).unwrap()
    }

    // #[test]
    // fn test_compression() {
    //     let dir = create_temp_dir("ChronoFileCompression");
    //     let mut file_path = dir.keep();
    //     file_path.push("compression-test.txt");

    //     let content = b"hello world";
    //     {
    //         let mut file = File::create_new(&file_path).unwrap();
    //         let _ = file.write(content).unwrap();
    //     }
    //     let file = File::open(file_path).unwrap();

    //     // compression
    //     let out = compress(&file);
    //     assert!(out.is_ok());
    //     let out = out.unwrap();
    //     assert!(!out.0.is_empty());
    //     assert!(out.1 > 0);

    //     // decompress
    //     let (compressed_data, _) = out;
    //     let decompressed = decompress(&compressed_data).unwrap();
    //     assert_eq!(&decompressed, content);
    // }

    // #[test]
    // fn test_large_file_compression() {
    //     let dir = create_temp_dir("ChronoFileCompression");
    //     let mut file_path = dir.keep();
    //     file_path.push("large-compression-test.txt");

    //     let content = vec![0u8; 1_048_576]; // 1MB of zeros
    //     {
    //         let mut file = File::create_new(&file_path).unwrap();
    //         let _ = file.write(&content).unwrap();
    //     }
    //     let file = File::open(file_path).unwrap();
    //     let buf = file.read();

    //     // compression
    //     let out = compress(&file);
    //     assert!(out.is_ok());
    //     let out = out.unwrap();
    //     assert!(!out.0.is_empty());
    //     assert!(out.1 > 0);

    //     // decompress
    //     let (compressed_data, _) = out;
    //     let decompressed = decompress(&compressed_data).unwrap();
    //     assert_eq!(decompressed, content);
    // }
}
