// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{
    ffi::OsString,
    fs::{File, Metadata, OpenOptions, Permissions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use uuid::Uuid;

use crate::{
    History,
    snapshot::{Snapshot, SnapshotData},
    utils::{convert_systemtime_to_uuid, read_all},
};

/// A file paired with a companion `.chrono` file.
///
/// [`ChronoFile`] is a drop-in for [`std::fs::File`]: its [`Read`], [`Write`]
/// and [`Seek`] impls pass straight through to the main file, byte for byte. Nothing is versioned automatically — **a version is created only when
/// you call [`commit`](History::commit)**.
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
    /// `OpenOptions::new().read(true).write(true).create(true).truncate(true)`.
    ///
    /// # Errors
    ///
    /// Returns an error if either the main file or the `.chrono` file cannot be
    /// opened. See [`OpenOptions::open`] for details.
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

    /// Changes the permissions on the main file. The `.chrono` file's
    /// permissions are left unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if the user lacks permission to change attributes on
    /// the main file. It may also return an error in other OS-specific cases.
    ///
    /// Note that this method alters the permissions of the main file, even
    /// though it takes `&self` rather than `&mut self`.
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
        SnapshotData::from_bytes(&read_all(&mut self.snapshot_file)?)
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
            // Verify the checksum before handing data back.
            snapshot.validate()?;
            Ok(Some(snapshot.to_vec()))
        } else {
            Ok(None)
        }
    }

    fn preview_at(&mut self, time: SystemTime) -> io::Result<Option<Vec<u8>>> {
        let id = convert_systemtime_to_uuid(time)?;
        self.preview(id)
    }

    fn restore(&mut self, version: Uuid) -> io::Result<Vec<u8>> {
        let snapshots = self.read_snapshot()?;
        let snapshot = snapshots.search(&version);
        match snapshot {
            Some(snapshot) => {
                // Verify the checksum before overwriting the main file.
                snapshot.validate()?;
                let vec = snapshot.to_vec();
                self.file.rewind()?;
                self.file.write_all(&vec)?;
                self.file.set_len(vec.len() as u64)?;
                self.commit()?;
                Ok(vec)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no version at or before the requested id",
            )),
        }
    }

    fn restore_at(&mut self, time: SystemTime) -> io::Result<Vec<u8>> {
        let id = convert_systemtime_to_uuid(time)?;
        self.restore(id)
    }

    fn commit(&mut self) -> io::Result<Option<Uuid>> {
        let snapshot = Snapshot::new(read_all(&mut self.file)?);
        let id = snapshot.id;

        // TODO: when called from restore(), snapshot data is read twice: once
        // there and once here.
        let mut snapshot_data = self.read_snapshot()?;

        if snapshot_data.last() == Some(&snapshot) {
            return Ok(None);
        }

        // Rewrite the whole history in place.
        snapshot_data.push(snapshot);
        let bytes = snapshot_data.to_vec()?;
        self.snapshot_file.rewind()?;
        self.snapshot_file.write_all(&bytes)?;
        self.snapshot_file.set_len(bytes.len() as u64)?;

        Ok(Some(id))
    }
}

impl Write for ChronoFile {
    /// Writes straight to the main file, exactly like [`std::fs::File`].
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    /// Flushes the main file.
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
    /// Seeks within the main file, exactly like [`std::fs::File`].
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
    fn create_truncates_existing_chrono() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        std::fs::write(&chrono_path, b"old history").unwrap();
        ChronoFile::create(&path).unwrap();

        assert_eq!(std::fs::read(&chrono_path).unwrap(), b"");
    }

    #[test]
    fn open_preserves_existing_file_and_chrono() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        std::fs::write(&path, b"main").unwrap();
        std::fs::write(&chrono_path, b"history").unwrap();

        ChronoFile::open(&path).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"main");
        assert_eq!(std::fs::read(&chrono_path).unwrap(), b"history");
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
    fn commit_and_restore_ignore_cursor_position() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let mut cf = ChronoFile::create(&path).unwrap();

        cf.write_all(b"hello world").unwrap();
        let first = cf.commit().unwrap().expect("first commit records");
        // commit leaves the main file and cursor alone
        assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
        assert_eq!(cf.stream_position().unwrap(), 11);

        cf.rewind().unwrap();
        cf.set_len(0).unwrap();
        cf.write_all(b"hi").unwrap();
        assert!(cf.commit().unwrap().is_some());
        assert!(
            cf.commit().unwrap().is_none(),
            "unchanged file records nothing"
        );

        let snaps = cf.list_snapshots().unwrap();
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].to_vec(), b"hello world");
        assert_eq!(snaps[1].to_vec(), b"hi");

        assert_eq!(cf.restore(first).unwrap(), b"hello world");
        assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
        assert_eq!(cf.list_snapshots().unwrap().len(), 3);
    }

    #[test]
    fn get_paths_appends_chrono_suffix() {
        for (input, chrono) in [
            ("dir/file.dat", "dir/file.dat.chrono"),
            ("file", "file.chrono"),
        ] {
            let (path, chrono_path) = ChronoFile::get_paths(input);

            assert_eq!(path, PathBuf::from(input));
            assert_eq!(chrono_path, OsString::from(chrono));
        }
    }
}
