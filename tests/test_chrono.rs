use std::io::Write;

use chronofile::ChronoFile;
use tempfile::TempDir;

pub fn create_temp_dir(prefix: &str) -> TempDir {
    TempDir::with_prefix(prefix).unwrap()
}

#[test]
fn test_write() {
    let dir = create_temp_dir("ChronoFileIntegrationTestBinary");
    let mut file_path = dir.keep();
    file_path.push("write.txt");

    // create file and write data
    {
        let mut file = ChronoFile::create_new(&file_path).unwrap();
        // let content = b"this is a very small piece of text which will be written into the file.";
        let content = vec![1 as u8; 1_048_576];
        let _length = file.write(&content).unwrap();
    }

    // append new data
    for i in 0..=4 {
        let mut file = ChronoFile::create(&file_path).unwrap();
        let content = vec![i as u8; 1_048_576]; // 1MB of i;
        let _length = file.write(&content).unwrap();
    }

    let mut file = ChronoFile::open(&file_path).unwrap();
    let versions = file.versions();
    dbg!(&versions);
    assert!(versions.is_ok());
    let versions = versions.unwrap();
    assert_eq!(versions.len(), 6);
}
