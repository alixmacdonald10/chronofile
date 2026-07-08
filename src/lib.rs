// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 A Macdonald

use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

mod patches;

use patches::{Patches, Select};

/// Current wall-clock time as milliseconds since the Unix epoch. Clocks set
/// before 1970 clamp to 0 rather than erroring — a version timestamp is
/// advisory metadata, never load-bearing.
fn now_ms() -> u64 {
    to_ms(std::time::SystemTime::now())
}

/// A [`SystemTime`](std::time::SystemTime) as milliseconds since the Unix
/// epoch. Times before 1970 clamp to 0.
fn to_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Metadata for one committed version, as returned by
/// [`list_versions`](History::list_versions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionInfo {
    /// Zero-based version id — pass this to [`restore`](History::restore) or
    /// [`preview`](History::preview).
    pub id: u64,
    /// Wall-clock time the version was committed.
    pub timestamp: std::time::SystemTime,
}

/// Read access to a file's committed version history.
pub trait History {
    /// Lists every committed version, oldest first, with its id and commit
    /// time so a caller can decide which one to [`preview`](History::preview)
    /// or [`restore`](History::restore).
    ///
    /// Returns an empty vec when nothing has been committed yet.
    fn list_versions(&mut self) -> std::io::Result<Vec<VersionInfo>>;

    /// Returns the file's contents as of `version` **without touching the main
    /// file** — a read-only peek to compare versions before restoring.
    ///
    /// `version` is a zero-based commit id (see
    /// [`list_versions`](History::list_versions)); the contents are
    /// reconstructed by replaying patches `0..=version`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the log cannot be read/replayed.
    fn preview(&mut self, version: u64) -> std::io::Result<Vec<u8>>;

    /// Like [`preview`](History::preview), but selects the version by time:
    /// the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the log cannot be
    /// read/replayed.
    fn preview_at(&mut self, time: std::time::SystemTime) -> std::io::Result<Vec<u8>>;

    /// Restores the file to its contents as of `version` and returns those
    /// bytes.
    ///
    /// `version` is a zero-based commit id (as returned by
    /// [`commit`](ChronoFile::commit)); the contents are reconstructed by
    /// replaying patches `0..=version`. The main file is rewound, overwritten
    /// with the restored bytes, and truncated to their length, so its contents
    /// afterwards match exactly.
    ///
    /// The restore is itself **recorded as a new version** and becomes the new
    /// snapshot baseline, so a subsequent [`commit`](ChronoFile::commit) diffs
    /// against the restored contents (not the pre-restore ones). Restoring the
    /// latest version is a no-op that records nothing.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if `version`
    /// does not exist, or an I/O error if the log cannot be read/replayed or
    /// the main file cannot be rewritten.
    fn restore(&mut self, version: u64) -> std::io::Result<Vec<u8>>;

    /// Like [`restore`](History::restore), but selects the version by time:
    /// restores the latest version committed **at or before** `time`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInput`](std::io::ErrorKind::InvalidInput) if no version
    /// was committed at or before `time`, or an I/O error if the log cannot be
    /// read/replayed or the main file cannot be rewritten.
    fn restore_at(&mut self, time: std::time::SystemTime) -> std::io::Result<Vec<u8>>;
}

/// A writable file that records its version history in a companion `.chrono`
/// file.
///
/// [`ChronoFile`] is a drop-in for [`std::fs::File`]: its [`Read`] and [`Write`]
/// impls pass straight through to the underlying file, byte for byte. Nothing
/// is versioned automatically.
///
/// # Versions are explicit
///
/// **A version is created only when you call [`commit`](ChronoFile::commit).**
/// Writing does *not* create a version — [`write`](std::io::Write::write) is a
/// low-level byte sink called many times per logical operation (`write_all`,
/// `writeln!`, [`io::copy`](std::io::copy)), so tying a version to it would mint
/// bogus half-written versions. Instead: write freely, then `commit()` when the
/// current contents are worth keeping.
///
/// ```no_run
/// use std::io::Write;
/// use chronofile::{ChronoFile, History};
///
/// let mut cf = ChronoFile::create("save.dat")?;
/// writeln!(cf, "level 1 complete")?;
/// cf.commit()?;                       // version 0
/// writeln!(cf, "level 2 complete")?;
/// cf.commit()?;                       // version 1
/// let v0 = cf.restore(0)?;            // contents at version 0
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct ChronoFile {
    file: std::fs::File,
    chrono: std::fs::File,
    /// Contents of the main file as of the last [`commit`](ChronoFile::commit),
    /// held in memory so a commit can diff against it without replaying the log.
    snapshot: Vec<u8>,
}

impl ChronoFile {
    /// Creates a new [`ChronoFile`], truncating both the main file and its
    /// companion `.chrono` file if they already exist.
    ///
    /// Both handles are opened for **reading and writing**. Read access is
    /// required even on creation because every [`write`] must first read the
    /// current contents of the main file (to diff against) and the existing
    /// patch log out of the `.chrono` file (to append to). This mirrors opening
    /// a [`std::fs::File`] with
    /// [`OpenOptions::read(true).write(true).create(true).truncate(true)`].
    ///
    /// # Errors
    ///
    /// This function will return an error if either the main file or the
    /// `.chrono` file cannot be opened. See [`OpenOptions::open`] for details.
    ///
    /// [`write`]: std::io::Write::write
    pub fn create<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let (path, chrono_path) = Self::get_paths(path);

        Ok(Self {
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?,
            chrono: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(chrono_path)?,
            snapshot: Vec::new(),
        })
    }

    /// Opens an existing [`ChronoFile`] without truncating it.
    ///
    /// The main file must already exist; it is opened for **reading and
    /// writing** so the returned handle satisfies both [`Read`] and [`Write`].
    /// The companion `.chrono` file is likewise opened read/write when present,
    /// and created (read/write) when missing so the first [`write`] has a patch
    /// log to append to.
    ///
    /// [`std::fs::File::open`] opens read-only; this differs deliberately
    /// because a [`ChronoFile`] must record a new patch on every write.
    ///
    /// # Errors
    ///
    /// Returns an error if the main file does not exist (kind
    /// [`NotFound`](std::io::ErrorKind::NotFound)) or if either file cannot be
    /// opened. When the main file is missing the `.chrono` file is **not**
    /// created.
    ///
    /// [`write`]: std::io::Write::write
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<ChronoFile> {
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

        let mut this = Self {
            file,
            chrono,
            snapshot: Vec::new(),
        };
        // rebuild the last-committed contents by replaying the patch log
        this.snapshot = this.replay_log()?;
        Ok(this)
    }

    fn get_paths<P: AsRef<Path>>(path: P) -> (PathBuf, OsString) {
        let path = path.as_ref();

        let mut chrono_path = OsString::from(path.as_os_str());
        chrono_path.push(".chrono");

        (path.to_owned(), chrono_path)
    }

    /// Reads the entire current contents of the main file.
    ///
    /// The cursor is rewound to the start first, so the full file is returned
    /// regardless of where a prior [`Read`]/[`Write`] left it. On return the
    /// cursor sits at end-of-file, so a subsequent [`std::fs::File::write`]
    /// appends. Unlike a bare [`std::fs::File::read`], this always yields the
    /// whole file.
    fn read_current_data(&mut self) -> std::io::Result<Vec<u8>> {
        self.file.rewind()?;
        let mut data = vec![];
        let _bytes = self.file.read_to_end(&mut data)?;
        Ok(data)
    }

    /// Reads the entire encoded patch log out of the `.chrono` file.
    ///
    /// The cursor is rewound to the start first because previous writes leave
    /// it at end-of-file; without the rewind [`std::fs::File::read_to_end`]
    /// would return zero bytes. The returned buffer is empty for a freshly
    /// created `.chrono` file.
    fn read_patch_log(&mut self) -> std::io::Result<Vec<u8>> {
        self.chrono.rewind()?;
        let mut data = vec![];
        let _bytes = self.chrono.read_to_end(&mut data)?;
        Ok(data)
    }

    /// Reads and decodes the `.chrono` patch log.
    ///
    /// An empty `.chrono` file (the state right after [`create`](ChronoFile::create))
    /// decodes to an empty log rather than erroring.
    fn load_patches(&mut self) -> std::io::Result<Patches> {
        let encoded = self.read_patch_log()?;
        Patches::decode(&encoded)
    }

    /// Serializes `patches` and rewrites the whole `.chrono` file from the start.
    ///
    /// The cursor is rewound and the file truncated with
    /// [`std::fs::File::set_len`] first so a shorter encoding cannot leave stale
    /// trailing bytes.
    fn write_log(&mut self, patches: &Patches) -> std::io::Result<()> {
        let encoded = patches.encode()?;
        self.chrono.rewind()?;
        self.chrono.set_len(0)?;
        self.chrono.write_all(&encoded)
    }

    /// Reconstructs the last-committed contents by replaying every patch in the
    /// log onto an empty buffer, in order.
    fn replay_log(&mut self) -> std::io::Result<Vec<u8>> {
        self.load_patches()?.replay()
    }

    /// Records the current contents of the main file as a new version and
    /// returns its version id (a zero-based, monotonically increasing index).
    ///
    /// This is the **only** way a version is created — see the
    /// [type-level docs](ChronoFile). Writing to the file changes its bytes but
    /// records nothing until you commit.
    ///
    /// The new version is stored as a diff against the previous commit, appended
    /// to the `.chrono` log.
    ///
    /// # No-op on no change
    ///
    /// If the file is byte-for-byte identical to the last commit, no version is
    /// created and `Ok(None)` is returned. Otherwise `Ok(Some(id))`.
    ///
    /// # Errors
    ///
    /// Returns an error if the main file cannot be read or the `.chrono` log
    /// cannot be encoded or written.
    pub fn commit(&mut self) -> std::io::Result<Option<u64>> {
        let current = self.read_current_data()?;

        // no-op: nothing changed since the last commit
        if current == self.snapshot {
            return Ok(None);
        }

        let patch_bytes = diffy::create_patch_bytes(&self.snapshot, &current).to_bytes();

        let mut patches = self.load_patches()?;
        patches.push(patch_bytes, now_ms());
        let id = patches.len() as u64 - 1;
        self.write_log(&patches)?;

        self.snapshot = current;
        Ok(Some(id))
    }
}

impl std::io::Write for ChronoFile {
    /// Writes straight to the underlying file, exactly like [`std::fs::File`].
    ///
    /// This does **not** create a version — call [`commit`](ChronoFile::commit)
    /// for that.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    /// Flushes the underlying file. Does not touch the version history; a
    /// version is only recorded by [`commit`](ChronoFile::commit).
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl std::io::Read for ChronoFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl History for ChronoFile {
    fn list_versions(&mut self) -> std::io::Result<Vec<VersionInfo>> {
        let patches = self.load_patches()?;
        Ok(patches
            .0
            .iter()
            .enumerate()
            .map(|(i, entry)| VersionInfo {
                id: i as u64,
                timestamp: std::time::UNIX_EPOCH
                    + std::time::Duration::from_millis(entry.timestamp_ms),
            })
            .collect())
    }

    fn preview(&mut self, version: u64) -> std::io::Result<Vec<u8>> {
        self.reconstruct(Select::Version(version as usize))
    }

    fn preview_at(&mut self, time: std::time::SystemTime) -> std::io::Result<Vec<u8>> {
        self.reconstruct(Select::AsOf(to_ms(time)))
    }

    fn restore(&mut self, version: u64) -> std::io::Result<Vec<u8>> {
        let data = self.reconstruct(Select::Version(version as usize))?;
        self.apply(data)
    }

    fn restore_at(&mut self, time: std::time::SystemTime) -> std::io::Result<Vec<u8>> {
        let data = self.reconstruct(Select::AsOf(to_ms(time)))?;
        self.apply(data)
    }
}

impl ChronoFile {
    /// Reconstructs the contents of the version picked by `select` by replaying
    /// its patches onto an empty buffer. Does **not** touch the main file.
    fn reconstruct(&mut self, select: Select) -> std::io::Result<Vec<u8>> {
        let all_patches = self.load_patches()?;
        match all_patches.filter(select) {
            Some(entries) => Patches(entries.to_vec()).replay(),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "no matching version found in .chrono".to_string(),
            )),
        }
    }

    /// Overwrites the main file with `data` (rewind + write + truncate) and
    /// commits it as a new version, returning `data`.
    fn apply(&mut self, data: Vec<u8>) -> std::io::Result<Vec<u8>> {
        self.file.rewind()?;
        self.file.write_all(&data)?;
        self.file.set_len(data.len() as u64)?;
        // record the restore so it becomes the new snapshot baseline
        self.commit()?;
        Ok(data)
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
        let err = match ChronoFile::open(&path) {
            Ok(_) => panic!("expected open to fail"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(!chrono_path.exists());
    }

    #[test]
    fn read_returns_file_contents() {
        use std::io::Read;

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
    fn patches_bincode_roundtrip() {
        let mut patches = Patches::default();
        patches.push(b"patch-one".to_vec(), 111);
        patches.push(b"patch-two".to_vec(), 222);

        let encoded = bincode2::serialize(&patches).unwrap();
        let decoded: Patches = bincode2::deserialize(&encoded[..]).unwrap();

        assert_eq!(patches, decoded);
    }

    #[test]
    fn read_patch_log_returns_chrono_contents() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        // seed a valid, replayable encoded log (open replays it on open, so
        // each patch must parse and apply from an empty base)
        let mut patches = Patches::default();
        patches.push(diffy::create_patch_bytes(b"", b"content\n").to_bytes(), 0);
        let encoded = bincode2::serialize(&patches).unwrap();

        std::fs::File::create(&path).unwrap();
        std::fs::write(&chrono_path, &encoded).unwrap();

        let mut cf = ChronoFile::open(&path).unwrap();

        let data = cf.read_patch_log().unwrap();
        assert_eq!(data, encoded);
    }

    #[test]
    fn commit_appends_patch_and_returns_id() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"hello").unwrap();

        assert_eq!(cf.commit().unwrap(), Some(0));

        let encoded = std::fs::read(&chrono_path).unwrap();
        let patches: Patches = bincode2::deserialize(&encoded[..]).unwrap();
        assert_eq!(patches.0.len(), 1);
    }

    #[test]
    fn commit_is_noop_when_unchanged() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let mut cf = ChronoFile::create(&path).unwrap();
        cf.write_all(b"hello").unwrap();

        assert_eq!(cf.commit().unwrap(), Some(0));
        // nothing written since => no new version
        assert_eq!(cf.commit().unwrap(), None);
    }

    #[test]
    fn many_writes_one_commit_records_single_version() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        let mut cf = ChronoFile::create(&path).unwrap();
        // writeln! calls write() several times, but only commit makes a version
        writeln!(cf, "line one").unwrap();
        writeln!(cf, "line two").unwrap();
        assert_eq!(cf.commit().unwrap(), Some(0));

        let encoded = std::fs::read(&chrono_path).unwrap();
        let patches: Patches = bincode2::deserialize(&encoded[..]).unwrap();
        assert_eq!(patches.0.len(), 1);
    }

    #[test]
    fn multiple_commits_record_ordered_patches() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");
        let chrono_path = tmp.path().join("file.dat.chrono");

        let mut cf = ChronoFile::create(&path).unwrap();

        // 6 versions; each appends a distinct chunk then commits. The file's
        // full contents at version i is the concatenation of chunks 0..=i.
        let mut expected: Vec<Vec<u8>> = Vec::new();
        let mut cumulative: Vec<u8> = Vec::new();
        for i in 0..6u64 {
            let chunk = format!("version {i} contents\n").into_bytes();
            cf.write_all(&chunk).unwrap();
            cumulative.extend_from_slice(&chunk);
            expected.push(cumulative.clone());

            assert_eq!(cf.commit().unwrap(), Some(i), "version id out of order");
        }

        let encoded = std::fs::read(&chrono_path).unwrap();
        let patches: Patches = bincode2::deserialize(&encoded[..]).unwrap();

        // one patch per commit
        assert_eq!(patches.0.len(), expected.len());

        // correct order: replaying patches 0..=i reconstructs version i's full
        // contents.
        let mut data: Vec<u8> = Vec::new();
        for (i, entry) in patches.0.iter().enumerate() {
            let patch = diffy::Patch::from_bytes(&entry.patch).unwrap();
            data = diffy::apply_bytes(&data, &patch).unwrap();
            assert_eq!(data, expected[i], "patch {i} out of order");
        }
    }

    /// Commits `n` versions, where version `i` has full contents
    /// `"version {i} contents\n"` repeated for `0..=i`. Returns the file path
    /// and the expected full contents at each version.
    fn seed_versions(dir: &Path, n: u64) -> (PathBuf, Vec<Vec<u8>>) {
        let path = dir.join("file.dat");
        let mut cf = ChronoFile::create(&path).unwrap();

        let mut expected = Vec::new();
        let mut cumulative = Vec::new();
        for i in 0..n {
            let chunk = format!("version {i} contents\n").into_bytes();
            cf.write_all(&chunk).unwrap();
            cumulative.extend_from_slice(&chunk);
            expected.push(cumulative.clone());
            assert_eq!(cf.commit().unwrap(), Some(i));
        }
        (path, expected)
    }

    #[test]
    fn list_versions_zero_when_empty() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let mut cf = ChronoFile::create(&path).unwrap();
        assert!(cf.list_versions().unwrap().is_empty());
    }

    #[test]
    fn list_versions_counts_commits() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();
        let versions = cf.list_versions().unwrap();
        assert_eq!(versions.len(), expected.len());
        // ids are the sequential 0..n
        assert_eq!(
            versions.iter().map(|v| v.id).collect::<Vec<_>>(),
            (0..expected.len() as u64).collect::<Vec<_>>()
        );
    }

    #[test]
    fn restore_returns_and_rewrites_intermediate_version() {
        use std::io::Read;

        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();

        // restore version 1 (contents = chunks 0..=1)
        let restored = cf.restore(1).unwrap();
        assert_eq!(restored, expected[1]);

        // the returned bytes must actually be on disk (shrunk from version 3)
        let mut buf = Vec::new();
        let mut file = std::fs::File::open(&path).unwrap();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, expected[1]);
    }

    #[test]
    fn restore_each_version_matches() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 5);

        let mut cf = ChronoFile::open(&path).unwrap();
        for (v, want) in expected.iter().enumerate() {
            assert_eq!(&cf.restore(v as u64).unwrap(), want, "version {v}");
        }
    }

    #[test]
    fn restore_records_new_version_and_updates_snapshot() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4); // versions 0..=3

        let mut cf = ChronoFile::open(&path).unwrap();
        assert_eq!(cf.list_versions().unwrap().len(), 4);

        // restoring an older version appends a new version recording it
        cf.restore(1).unwrap();
        assert_eq!(cf.list_versions().unwrap().len(), 5);

        // snapshot now == restored contents, so an immediate commit is a no-op
        assert_eq!(cf.commit().unwrap(), None);

        // the newest version reconstructs to the restored contents
        assert_eq!(cf.restore(4).unwrap(), expected[1]);
    }

    #[test]
    fn restore_latest_is_noop() {
        let tmp = tempdir().unwrap();
        let (path, _expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();
        // restoring the current latest version changes nothing => no new version
        cf.restore(3).unwrap();
        assert_eq!(cf.list_versions().unwrap().len(), 4);
    }

    #[test]
    fn list_versions_reports_timestamps_in_range() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("file.dat");

        let before = std::time::SystemTime::now();
        let (path, _expected) = {
            let mut cf = ChronoFile::create(&path).unwrap();
            cf.write_all(b"one").unwrap();
            cf.commit().unwrap();
            (path, ())
        };
        let after = std::time::SystemTime::now();

        let mut cf = ChronoFile::open(&path).unwrap();
        let versions = cf.list_versions().unwrap();
        assert_eq!(versions.len(), 1);
        // commit time falls between the two wall-clock samples (millisecond
        // resolution, so allow equality)
        assert!(versions[0].timestamp >= before - std::time::Duration::from_millis(1));
        assert!(versions[0].timestamp <= after + std::time::Duration::from_millis(1));
    }

    #[test]
    fn preview_does_not_touch_main_file() {
        use std::io::Read;

        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();

        // peek at an old version
        assert_eq!(cf.preview(1).unwrap(), expected[1]);
        // no new version recorded and the file still holds the latest contents
        assert_eq!(cf.list_versions().unwrap().len(), 4);

        let mut buf = Vec::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, expected[3]);
    }

    #[test]
    fn preview_out_of_range_errors() {
        let tmp = tempdir().unwrap();
        let (path, _expected) = seed_versions(tmp.path(), 3);

        let mut cf = ChronoFile::open(&path).unwrap();
        let err = cf.preview(99).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn preview_at_future_returns_latest() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        // "as of the future" resolves to the newest version
        assert_eq!(cf.preview_at(future).unwrap(), *expected.last().unwrap());
    }

    #[test]
    fn preview_at_commit_time_reconstructs_that_version_or_later() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();
        let versions = cf.list_versions().unwrap();

        // "as of version i's commit time" reconstructs version i — or a later
        // one if several commits share the same millisecond (rposition picks
        // the latest at-or-before). So the result must be some expected[j>=i].
        for (i, v) in versions.iter().enumerate() {
            let got = cf.preview_at(v.timestamp).unwrap();
            assert!(
                expected[i..].contains(&got),
                "as-of version {i} gave contents outside expected[{i}..]"
            );
        }
    }

    #[test]
    fn preview_at_before_history_errors() {
        let tmp = tempdir().unwrap();
        let (path, _expected) = seed_versions(tmp.path(), 3);

        let mut cf = ChronoFile::open(&path).unwrap();
        // nothing was committed at the Unix epoch
        let err = cf.preview_at(std::time::UNIX_EPOCH).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn restore_at_future_restores_and_records_latest() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 4);

        let mut cf = ChronoFile::open(&path).unwrap();
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);

        let restored = cf.restore_at(future).unwrap();
        assert_eq!(restored, *expected.last().unwrap());
        // restoring the already-latest contents is a no-op commit
        assert_eq!(cf.list_versions().unwrap().len(), 4);
    }

    #[test]
    fn restore_out_of_range_errors() {
        let tmp = tempdir().unwrap();
        let (path, expected) = seed_versions(tmp.path(), 3);

        let mut cf = ChronoFile::open(&path).unwrap();
        let err = cf.restore(expected.len() as u64).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
