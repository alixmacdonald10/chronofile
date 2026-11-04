// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

//! End-to-end tests exercising `ChronoFile` through its public API only, the
//! way an external crate would use it: create/open and the `std::fs::File`
//! parity surface (`Read`/`Write`/`Seek`, `set_len`, metadata, sync), plus
//! the `History` versioning contract documented in `lib.rs`.

use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use chronofile::{ChronoFile, History};
use tempfile::{TempDir, tempdir};

fn new_file() -> (TempDir, PathBuf, ChronoFile) {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");
    let cf = ChronoFile::create(&path).unwrap();
    (tmp, path, cf)
}

/// Replace the whole main file with `bytes`, then commit.
fn write_and_commit(cf: &mut ChronoFile, bytes: &[u8]) -> Option<uuid::Uuid> {
    cf.rewind().unwrap();
    cf.set_len(0).unwrap();
    cf.write_all(bytes).unwrap();
    cf.commit().unwrap()
}

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

fn commit_time(id: uuid::Uuid) -> SystemTime {
    let (secs, nanos) = id.get_timestamp().unwrap().to_unix();
    SystemTime::UNIX_EPOCH + Duration::new(secs, nanos)
}

// ---- History ----

#[test]
fn list_snapshots_empty_on_new_file() {
    let (_tmp, _path, mut cf) = new_file();
    assert!(cf.list_snapshots().unwrap().is_empty());
}

#[test]
fn commit_then_list_snapshots_oldest_first() {
    let (_tmp, _path, mut cf) = new_file();
    let v1 = write_and_commit(&mut cf, b"one").unwrap();
    let v2 = write_and_commit(&mut cf, b"two").unwrap();

    let ids: Vec<_> = cf.list_snapshots().unwrap().iter().map(|s| s.id).collect();
    assert_eq!(ids, [v1, v2]);
}

#[test]
fn commits_survive_reopen() {
    let (_tmp, path, mut cf) = new_file();
    let v1 = write_and_commit(&mut cf, b"one").unwrap();
    drop(cf);

    let mut cf = ChronoFile::open(&path).unwrap();
    assert_eq!(cf.preview(v1).unwrap().as_deref(), Some(&b"one"[..]));
}

#[test]
fn commit_after_write_captures_whole_file() {
    // No rewind before commit: the cursor sits at EOF after writing.
    let (_tmp, _path, mut cf) = new_file();
    cf.write_all(b"full contents").unwrap();
    let v1 = cf.commit().unwrap().unwrap();

    assert_eq!(
        cf.preview(v1).unwrap().as_deref(),
        Some(&b"full contents"[..])
    );
}

#[test]
fn commit_unchanged_content_returns_none() {
    let (_tmp, _path, mut cf) = new_file();
    assert!(write_and_commit(&mut cf, b"same").is_some());
    assert!(cf.commit().unwrap().is_none());
    assert_eq!(cf.list_snapshots().unwrap().len(), 1);
}

#[test]
fn commit_dedups_against_latest_not_first() {
    // A, B, A: the second A differs from the latest (B), so it is a new version.
    let (_tmp, _path, mut cf) = new_file();
    write_and_commit(&mut cf, b"A").unwrap();
    write_and_commit(&mut cf, b"B").unwrap();
    assert!(write_and_commit(&mut cf, b"A").is_some());
    assert_eq!(cf.list_snapshots().unwrap().len(), 3);
}

#[test]
fn preview_does_not_touch_main_file() {
    let (_tmp, path, mut cf) = new_file();
    let v1 = write_and_commit(&mut cf, b"old").unwrap();
    write_and_commit(&mut cf, b"new").unwrap();

    assert_eq!(cf.preview(v1).unwrap().as_deref(), Some(&b"old"[..]));
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
}

#[test]
fn preview_before_first_commit_is_none() {
    let (_tmp, _path, mut cf) = new_file();
    let before = uuid::Uuid::now_v7();
    std::thread::sleep(Duration::from_millis(2));
    write_and_commit(&mut cf, b"one").unwrap();

    assert_eq!(cf.preview(before).unwrap(), None);
}

#[test]
fn restore_overwrites_and_truncates() {
    let (_tmp, path, mut cf) = new_file();
    let long = write_and_commit(&mut cf, b"a long first version").unwrap();
    let short = write_and_commit(&mut cf, b"short").unwrap();

    assert_eq!(cf.restore(long).unwrap(), b"a long first version");
    assert_eq!(std::fs::read(&path).unwrap(), b"a long first version");

    assert_eq!(cf.restore(short).unwrap(), b"short");
    assert_eq!(std::fs::read(&path).unwrap(), b"short");
}

#[test]
fn restore_records_new_version() {
    let (_tmp, _path, mut cf) = new_file();
    let v1 = write_and_commit(&mut cf, b"one").unwrap();
    write_and_commit(&mut cf, b"two").unwrap();

    cf.restore(v1).unwrap();
    let snapshots = cf.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        cf.preview(snapshots[2].id).unwrap().as_deref(),
        Some(&b"one"[..])
    );
}

#[test]
fn restore_latest_is_noop() {
    let (_tmp, _path, mut cf) = new_file();
    write_and_commit(&mut cf, b"one").unwrap();
    let v2 = write_and_commit(&mut cf, b"two").unwrap();

    cf.restore(v2).unwrap();
    assert_eq!(cf.list_snapshots().unwrap().len(), 2);
}

#[test]
fn restore_before_first_commit_is_invalid_input() {
    // Docs promise InvalidInput; an empty Vec would be indistinguishable from an empty file.
    let (_tmp, _path, mut cf) = new_file();
    let before = uuid::Uuid::now_v7();
    std::thread::sleep(Duration::from_millis(2));
    write_and_commit(&mut cf, b"one").unwrap();

    assert_eq!(
        cf.restore(before).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
}

#[test]
fn preview_at_and_restore_at_select_by_time() {
    let (_tmp, path, mut cf) = new_file();
    let v1 = write_and_commit(&mut cf, b"one").unwrap();
    std::thread::sleep(Duration::from_millis(2));
    write_and_commit(&mut cf, b"two").unwrap();

    let an_hour_ago = SystemTime::now() - Duration::from_secs(3600);
    assert_eq!(cf.preview_at(an_hour_ago).unwrap(), None);
    // Exact commit time must resolve to that commit, even within the same millisecond.
    assert_eq!(
        cf.preview_at(commit_time(v1)).unwrap().as_deref(),
        Some(&b"one"[..])
    );
    assert_eq!(
        cf.preview_at(SystemTime::now()).unwrap().as_deref(),
        Some(&b"two"[..])
    );

    assert_eq!(cf.restore_at(commit_time(v1)).unwrap(), b"one");
    assert_eq!(std::fs::read(&path).unwrap(), b"one");
}

#[test]
fn corrupt_chrono_is_invalid_data() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");
    std::fs::write(&path, b"main").unwrap();
    std::fs::write(tmp.path().join("doc.txt.chrono"), [0xff; 16]).unwrap();

    let mut cf = ChronoFile::open(&path).unwrap();
    assert_eq!(
        cf.list_snapshots().unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}
