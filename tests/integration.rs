// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

//! End-to-end tests exercising `ChronoFile` through its public API only, the
//! way an external crate would use it: create/open, the `Read`/`Write`/`Seek`
//! impls, `commit`, and the `History` trait.

use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{Duration, SystemTime};

use chronofile::{ChronoFile, History};
use tempfile::tempdir;

/// create → write → commit → read back the working copy.
#[test]
fn create_write_commit_roundtrip() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("save.dat");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"hello chrono").unwrap();

    // first commit is version 0
    assert_eq!(cf.commit().unwrap(), Some(0));

    // the .chrono companion sits next to the main file
    assert!(path.exists());
    assert!(tmp.path().join("save.dat.chrono").exists());

    // reading rewinds and returns the full working copy
    cf.rewind().unwrap();
    let mut buf = Vec::new();
    cf.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"hello chrono");
}

/// commit with no change since the last one records nothing.
#[test]
fn commit_is_noop_without_changes() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("save.dat");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"data").unwrap();

    assert_eq!(cf.commit().unwrap(), Some(0));
    assert_eq!(cf.commit().unwrap(), None); // nothing written since
    assert_eq!(cf.list_versions().unwrap().len(), 1);
}

/// Many small writes then a single commit yield exactly one version.
#[test]
fn many_writes_one_commit() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("log.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    writeln!(cf, "line one").unwrap();
    writeln!(cf, "line two").unwrap();
    write!(cf, "no newline").unwrap();

    assert_eq!(cf.commit().unwrap(), Some(0));
    assert_eq!(cf.list_versions().unwrap().len(), 1);
}

/// History survives dropping the handle and reopening the file.
#[test]
fn history_persists_across_reopen() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("save.dat");

    {
        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"v0").unwrap();
        cf.commit().unwrap();
        cf.write_all(b"-v1").unwrap();
        cf.commit().unwrap();
    } // handle dropped

    let mut cf = ChronoFile::open(&path).unwrap();
    let versions = cf.list_versions().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions.iter().map(|v| v.id).collect::<Vec<_>>(), vec![0, 1]);

    // an immediate commit is a no-op: reopening rebuilt the snapshot baseline
    assert_eq!(cf.commit().unwrap(), None);
}

/// Full lifecycle: build several versions, restore an old one, confirm the
/// working copy on disk is rewritten and the restore is itself a new version.
#[test]
fn restore_rewrites_working_copy_and_records_version() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"first").unwrap();
    cf.commit().unwrap(); // v0 = "first"
    cf.write_all(b"-second").unwrap();
    cf.commit().unwrap(); // v1 = "first-second"
    cf.write_all(b"-third").unwrap();
    cf.commit().unwrap(); // v2 = "first-second-third"

    // restore v0 returns its bytes...
    let restored = cf.restore(0).unwrap();
    assert_eq!(restored, b"first");

    // ...and rewrites + truncates the actual file on disk
    let on_disk = std::fs::read(&path).unwrap();
    assert_eq!(on_disk, b"first");

    // restore recorded a new version (v3), so there are now four
    assert_eq!(cf.list_versions().unwrap().len(), 4);
    // and the snapshot baseline is the restored contents => no-op commit
    assert_eq!(cf.commit().unwrap(), None);
}

/// preview reconstructs an old version without touching the working copy.
#[test]
fn preview_does_not_mutate_working_copy() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"alpha").unwrap();
    cf.commit().unwrap(); // v0
    cf.write_all(b"-beta").unwrap();
    cf.commit().unwrap(); // v1

    assert_eq!(cf.preview(0).unwrap(), b"alpha");

    // no new version, file still holds the latest bytes
    assert_eq!(cf.list_versions().unwrap().len(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), b"alpha-beta");
}

/// Out-of-range selectors surface as InvalidInput, not a panic.
#[test]
fn out_of_range_version_errors() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"only").unwrap();
    cf.commit().unwrap();

    let err = cf.preview(99).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    let err = cf.restore(99).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

/// Time-travel by timestamp: `preview_at`/`restore_at` resolve to the latest
/// version at or before the given time.
#[test]
fn time_travel_by_timestamp() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"start").unwrap();
    cf.commit().unwrap();

    let versions = cf.list_versions().unwrap();
    let commit_time = versions[0].timestamp;

    // as of the commit time => that version's contents
    assert_eq!(cf.preview_at(commit_time).unwrap(), b"start");

    // far in the future => newest version
    let future = SystemTime::now() + Duration::from_secs(3600);
    assert_eq!(cf.preview_at(future).unwrap(), b"start");

    // before any commit => InvalidInput
    let err = cf.preview_at(SystemTime::UNIX_EPOCH).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    // restore_at resolves the same way and rewrites the working copy
    let restored = cf.restore_at(future).unwrap();
    assert_eq!(restored, b"start");
}

/// seek + overwrite mid-file, then commit captures the edited bytes.
#[test]
fn seek_overwrite_then_commit() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"AAAAA").unwrap();
    cf.commit().unwrap(); // v0 = "AAAAA"

    // overwrite the middle byte
    cf.seek(SeekFrom::Start(2)).unwrap();
    cf.write_all(b"B").unwrap();
    let id = cf.commit().unwrap().expect("edit is a change");

    assert_eq!(cf.preview(id).unwrap(), b"AABAA");
    assert_eq!(cf.preview(0).unwrap(), b"AAAAA");
}

/// set_len truncation and extension are ordinary byte changes a commit records.
#[test]
fn set_len_changes_are_versioned() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"abcdef").unwrap();
    cf.commit().unwrap(); // v0 = "abcdef"

    cf.set_len(3).unwrap(); // truncate
    let truncated = cf.commit().unwrap().expect("truncation is a change");
    assert_eq!(cf.preview(truncated).unwrap(), b"abc");

    cf.set_len(5).unwrap(); // extend with zero bytes
    let extended = cf.commit().unwrap().expect("extension is a change");
    assert_eq!(cf.preview(extended).unwrap(), b"abc\0\0");
}

/// The two metadata accessors target the main file and the .chrono log
/// separately, and both sync variants are callable.
#[test]
fn metadata_and_sync() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("doc.txt");

    let mut cf = ChronoFile::create(&path).unwrap();
    cf.write_all(b"contents-of-the-main-file").unwrap();
    cf.commit().unwrap();

    let main = cf.metadata().unwrap();
    let chrono = cf.chrono_metadata().unwrap();
    assert!(main.is_file());
    assert!(chrono.is_file());

    // durability calls succeed
    cf.sync_data().unwrap();
    cf.sync_all().unwrap();
}

/// A fresh ChronoFile has no versions.
#[test]
fn new_file_has_no_versions() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("empty.dat");

    let mut cf = ChronoFile::create(&path).unwrap();
    assert!(cf.list_versions().unwrap().is_empty());
}
