use std::{io::{Write, Read}, time::{Duration, SystemTime}};

use chronofile::ChronoFile;
use tempfile::TempDir;

pub fn create_temp_dir(prefix: &str) -> TempDir {
    TempDir::with_prefix(prefix).unwrap()
}

#[test]
fn test() {
    let dir = create_temp_dir("ChronoFileIntegrationTestBinary");
    let mut file_path = dir.keep();
    file_path.push("write.txt");

    let text = "This is expected";
    // create file and write data
    {
        let mut file = ChronoFile::create_new(&file_path).unwrap();
        // let content = b"this is a very small piece of text which will be written into the file.";
        let content = text.to_string().as_bytes().to_vec();
        let _length = file.write(&content).unwrap();
    }

    let initial_write_time = SystemTime::now();

    // test appending new data
    for i in 0..=4 {
        std::thread::sleep(Duration::from_secs(1));
        let mut file = ChronoFile::create(&file_path).unwrap();
        let content = vec![i as u8; 1_048_576]; // 1MB of i;
        let _length = file.write(&content).unwrap();
    }
    {
        let mut file = ChronoFile::open(&file_path).unwrap();
        let versions = file.versions();
        assert!(versions.is_ok());
        let versions = versions.unwrap();
        assert_eq!(versions.len(), 6);
    }

    // test restoring files
    {
        let mut file = ChronoFile::create(&file_path).unwrap();
        let bytes = file.restore(initial_write_time);
        assert!(bytes.is_ok());
    }
    {
        let mut file = ChronoFile::open(&file_path).unwrap();
        let mut buf = Vec::new();
        let bytes = file.read_to_end(&mut buf);
        assert!(bytes.is_ok());

        let output = String::from_utf8(buf).unwrap();
        let expected_output = text.to_string();
        dbg!(&output);
        dbg!(&expected_output);
        assert!(output == expected_output);
    }

}
