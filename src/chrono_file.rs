// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{
    ffi::OsString,
    fs::{File, Metadata, OpenOptions, Permissions},
    io::{self, Error, ErrorKind, Read, Seek, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use uuid::{Timestamp, Uuid};

use crate::{
    History,
    snapshot::{Snapshot, SnapshotData},
    utils::convert_systemtime_to_uuid,
};

/// A file paired with a companion `.chrono` file.
///
/// [`ChronoFile`] is a drop-in for [`std::fs::File`]: its [`Read`], [`Write`]
/// and [`Seek`] impls pass straight through to the underlying file, byte for
/// byte. Nothing is versioned automatically — **a version is created only when
/// you call [`commit`](ChronoFile::commit)**.
///
/// ```no_run
/// use std::io::{Read, Seek, Write};
/// use chronofile::ChronoFile;
///
/// let mut cf = ChronoFile::create("save.dat")?;
/// writeln!(cf, "level 1 complete")?;
/// cf.rewind()?;
/// let mut buf = String::new();
/// cf.read_to_string(&mut buf)?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct ChronoFile {
    file: File,
    snapshot_file: File,
}

impl ChronoFile {
    /// Creates a new [`ChronoFile`], truncating both the main file and its
    /// companion `.chrono` file if they already exist.
    ///
    /// Both handles are opened for **reading and writing**, mirroring a
    /// [`std::fs::File`] opened with
    /// [`OpenOptions::read(true).write(true).create(true).truncate(true)`].
    ///
    /// # Errors
    ///
    /// This function will return an error if either the main file or the
    /// `.chrono` file cannot be opened. See [`OpenOptions::open`] for details.
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let (path, chrono_path) = Self::get_paths(path);

        Ok(Self {
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?,
            snapshot_file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(chrono_path)?,
        })
    }

    /// Opens an existing [`ChronoFile`] without truncating it.
    ///
    /// The main file must already exist; it is opened for **reading and
    /// writing** so the returned handle satisfies both [`Read`] and [`Write`].
    /// The companion `.chrono` file is likewise opened read/write when present,
    /// and created (read/write) when missing.
    ///
    /// [`std::fs::File::open`] opens read-only; this differs deliberately so
    /// a [`ChronoFile`] can always record new versions.
    ///
    /// # Errors
    ///
    /// Returns an error if the main file does not exist (kind
    /// [`NotFound`](std::io::ErrorKind::NotFound)) or if either file cannot be
    /// opened. When the main file is missing the `.chrono` file is **not**
    /// created.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<ChronoFile> {
        let (path, chrono_path) = Self::get_paths(path);

        let file = OpenOptions::new().read(true).write(true).open(path)?;

        let chrono = match std::fs::exists(&chrono_path) {
            Ok(true) => OpenOptions::new().read(true).write(true).open(chrono_path),
            Ok(false) => OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(chrono_path),
            Err(err) => return Err(err),
        }?;

        Ok(Self {
            file,
            snapshot_file: chrono,
        })
    }

    fn get_paths<P: AsRef<Path>>(path: P) -> (PathBuf, OsString) {
        let path = path.as_ref();

        let mut chrono_path = OsString::from(path.as_os_str());
        chrono_path.push(".chrono");

        (path.to_owned(), chrono_path)
    }

    /// Returns filesystem [`Metadata`] for the main file (the current,
    /// working copy that reads and writes act on).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the main file's metadata cannot be read.
    pub fn metadata(&self) -> io::Result<Metadata> {
        self.file.metadata()
    }

    /// Changes the permissions on the underlying file, chronofile permissions remain.
    ///
    /// # Errors
    ///
    /// This function will return an error if the user lacks permission change
    /// attributes on the underlying file. It may also return an error in other
    /// os-specific unspecified cases.
    ///
    /// Note that this method alters the permissions of the underlying file,
    /// even though it takes `&self` rather than `&mut self`.
    pub fn set_permissions(&self, perm: Permissions) -> io::Result<()> {
        self.file.set_permissions(perm)
    }

    /// Returns filesystem [`Metadata`] for the companion `.chrono` file.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the `.chrono` file's metadata cannot be read.
    pub fn chrono_metadata(&self) -> io::Result<Metadata> {
        self.snapshot_file.metadata()
    }

    /// Flushes all in-memory data and metadata for both files to disk.
    ///
    /// The `.chrono` file is synced first, then the main file. Returns once
    /// both files are durable.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if either file cannot be synced to disk.
    pub fn sync_all(&self) -> io::Result<()> {
        self.snapshot_file.sync_all()?;
        self.file.sync_all()?;
        Ok(())
    }

    /// Flushes in-memory data for both files to disk, skipping metadata that
    /// is not required to read the data back (see [`std::fs::File::sync_data`]).
    ///
    /// The `.chrono` file is synced first, as in [`Self::sync_all`].
    ///
    /// # Errors
    ///
    /// Returns an I/O error if either file's data cannot be synced to disk.
    pub fn sync_data(&self) -> io::Result<()> {
        self.snapshot_file.sync_data()?;
        self.file.sync_data()?;
        Ok(())
    }

    /// Truncates or extends the main file, exactly like
    /// [`std::fs::File::set_len`]. The `.chrono` file is untouched.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the main file cannot be resized.
    pub fn set_len(&self, size: u64) -> io::Result<()> {
        self.file.set_len(size)
    }

    fn read_snapshot(&mut self) -> io::Result<SnapshotData> {
        let mut buffer: Vec<u8> = Vec::new();
        self.snapshot_file.read_to_end(&mut buffer)?;
        let (snapshot_data, length): (SnapshotData, usize) =
            bincode_next::serde::decode_from_slice(&buffer[..], bincode_next::config::standard())
                .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;
        Ok(snapshot_data)
    }
}

impl History for ChronoFile {
    fn list_snapshots(&mut self) -> io::Result<Vec<Snapshot>> {
        let snapshots = self.read_snapshot()?;
        Ok(snapshots.into_iter().collect::<Vec<Snapshot>>())
    }

    fn preview(&mut self, version: Uuid) -> io::Result<Option<Vec<u8>>> {
        let snapshots = self.read_snapshot()?;
        if let Some(snapshot) = snapshots.search(&version) {
            return Ok(Some(snapshot.to_vec()));
        } else {
            return Ok(None);
        }
    }

    fn preview_at(&mut self, time: SystemTime) -> io::Result<Option<Vec<u8>>> {
        let id = convert_systemtime_to_uuid(time)?;
        let snapshots = self.read_snapshot()?;
        if let Some(snapshot) = snapshots.search(&id) {
            return Ok(Some(snapshot.to_vec()));
        } else {
            return Ok(None);
        }
    }

    fn restore(&mut self, version: Uuid) -> io::Result<Vec<u8>> {
        // let snapshots = self.read_snapshot()?;
        // if let Some(snapshot)
        //
        // // restore files
        // // commit
        // // TODO: how can we write file and commit in a single transaction?
        // // if there is any error we want a rollback
        // self.commit()?;
        todo!()
    }

    fn restore_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>> {
        // get version
        let id = convert_systemtime_to_uuid(time)?;
        // restore files
        // commit
        self.commit()?;
        todo!()
    }

    fn commit(&mut self) -> io::Result<Option<Uuid>> {
        // get current file data
        // create snapshot
        // append to data
        // save data
        todo!()
    }
}

impl Write for ChronoFile {
    /// Writes straight to the underlying file, exactly like [`std::fs::File`].
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    /// Flushes the underlying file.
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Read for ChronoFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for ChronoFile {
    /// Seeks within the underlying file, exactly like [`std::fs::File`].
    ///
    /// This moves the shared file cursor that [`Read`] and [`Write`] use. The
    /// [`Seek`] trait's other methods ([`rewind`](std::io::Seek::rewind),
    /// [`stream_position`](std::io::Seek::stream_position)) come for free —
    /// they default to calling this.
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn create_file() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        ChronoFile::create(&path).unwrap();

        assert!(path.exists());
        assert!(chrono_path.exists());
    }

    #[test]
    fn create_file_fails_with_chrono_preexisting() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        std::fs::File::create(&chrono_path).unwrap();
        ChronoFile::create(&path).unwrap();

        assert!(path.exists());
        assert!(chrono_path.exists());
    }

    #[test]
    fn open_existing_file_and_chrono() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        std::fs::File::create(&path).unwrap();
        std::fs::File::create(&chrono_path).unwrap();

        ChronoFile::open(&path).unwrap();
    }

    #[test]
    fn open_creates_chrono_when_missing() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        std::fs::File::create(&path).unwrap();
        assert!(!chrono_path.exists());

        ChronoFile::open(&path).unwrap();

        assert!(chrono_path.exists());
    }

    #[test]
    fn open_fails_when_file_missing() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let chrono_path = tmp.path().join("file.dat.chrono");

        // File missing: open fails and chrono must NOT be created.
        let Err(err) = ChronoFile::open(&path) else {
            panic!("expected open to fail");
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(!chrono_path.exists());
    }

    #[test]
    fn read_returns_file_contents() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        std::fs::write(&path, b"hello chrono").unwrap();

        let mut cf = ChronoFile::open(&path).unwrap();

        let mut buf = Vec::new();
        cf.read_to_end(&mut buf).unwrap();

        assert_eq!(buf, b"hello chrono");
    }

    #[test]
    fn get_paths_appends_chrono_suffix() {
        let (path, chrono_path) = ChronoFile::get_paths("dir/file.dat");

        assert_eq!(path, PathBuf::from("dir/file.dat"));
        assert_eq!(chrono_path, OsString::from("dir/file.dat.chrono"));
    }

    #[test]
    fn get_paths_no_extension() {
        let (path, chrono_path) = ChronoFile::get_paths("file");

        assert_eq!(path, PathBuf::from("file"));
        assert_eq!(chrono_path, OsString::from("file.chrono"));
    }

    #[test]
    fn metadata_and_chrono_metadata_target_separate_files() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        // Write only to the main file so the two files differ in size,
        // proving chrono_metadata reads the .chrono file, not the main file.
        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"main-file-contents").unwrap();

        let main = cf.metadata().unwrap();
        let chrono = cf.chrono_metadata().unwrap();

        assert!(main.is_file());
        assert!(chrono.is_file());
        assert!(main.len() > 0);
        assert_eq!(chrono.len(), 0);
    }

    #[test]
    fn set_len_truncates() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"some contents").unwrap();
        cf.set_len(0).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn set_len_extends_with_zero_fill() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"abc").unwrap();
        cf.set_len(6).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"abc\0\0\0");
    }
}
