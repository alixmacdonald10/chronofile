use std::io::Write;

use chronofile::ChronoFile;
use tempfile::TempDir;

pub fn create_temp_dir(prefix: &str) -> TempDir {
    TempDir::with_prefix(prefix).unwrap()
}

#[test]
fn test_write() {
    let dir = create_temp_dir("ChronoFileIntegrationTest");
    let mut file_path = dir.keep();
    file_path.push("write.txt");

    // create file and write data
    {
        let mut file = ChronoFile::create_new(&file_path).unwrap();
        let content = b"this is a very small piece of text which will be written into the file.";
        let _length = file.write(content).unwrap();
    }
    // append new data
    for i in 0..=5 {
        let mut file = ChronoFile::create(&file_path).unwrap();
        let content = vec![i as u8; 1_048_576]; // 1MB of i;
        let _length = file.write(&content).unwrap();
    }
}
