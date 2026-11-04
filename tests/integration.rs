// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

//! End-to-end tests exercising `ChronoFile` through its public API only, the
//! way an external crate would use it: create/open and the `std::fs::File`
//! parity surface (`Read`/`Write`/`Seek`, `set_len`, metadata, sync).

use std::io::{Read, Seek, SeekFrom, Write};

use chronofile::ChronoFile;
use tempfile::tempdir;

/// create → write → drop → reopen → read back the contents.
#[test]
fn create_write_reopen_read_roundtrip() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    {
        let mut cf = ChronoFile::create(&path).unwrap();
        writeln!(cf, "hello").unwrap();
        writeln!(cf, "world").unwrap();
    }

    let mut cf = ChronoFile::open(&path).unwrap();
    let mut buf = String::new();
    cf.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "hello\nworld\n");
}

/// seek + overwrite mid-file changes only the targeted byte.
#[test]
fn seek_overwrite_then_read() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"AAAAA").unwrap();

    cf.seek(SeekFrom::Start(2)).unwrap();
    cf.write_all(b"B").unwrap();

    cf.rewind().unwrap();
    let mut buf = Vec::new();
    cf.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"AABAA");
}

/// `set_len` truncates and extends (zero-filled) like `std::fs::File`.
#[test]
fn set_len_truncate_and_extend() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"abcdef").unwrap();

    cf.set_len(3).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc");

    cf.set_len(5).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abc\0\0");
}

/// The two metadata accessors target the main file and the .chrono file
/// separately, and both sync variants are callable.
#[test]
fn metadata_and_sync() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"contents-of-the-main-file").unwrap();

    let main = cf.metadata().unwrap();
    let chrono = cf.chrono_metadata().unwrap();
    assert!(main.is_file());
    assert!(chrono.is_file());
    assert_ne!(main.len(), chrono.len());

    cf.sync_data().unwrap();
    cf.sync_all().unwrap();
}
