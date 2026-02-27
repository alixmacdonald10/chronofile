//! Temporally versioned File manipulation operations.
//!
//! This module contains basic methods to manipulate a file which has automatic
//! temporal versioning which allows time travel and reverting.
//!
//! All methods in this module represent cross-platform filesystem
//! operations and are based on the Rust standard library [std::fs::File] operations.
//!
//! # Time of Check to Time of Use (TOCTOU)
//!
//! Many filesystem operations are subject to a race condition known as "Time of Check to Time of Use"
//! (TOCTOU). This occurs when a program checks a condition (like file existence or permissions)
//! and then uses the result of that check to make a decision, but the condition may have changed
//! between the check and the use.
//!
//! For example, checking if a file exists and then creating it if it doesn't is vulnerable to
//! TOCTOU - another process could create the file between your check and creation attempt.
//!
//! To avoid TOCTOU issues:
//! - Be aware that metadata operations (like [`metadata`] or [`symlink_metadata`]) may be affected by
//! changes made by other processes.
//! - Use atomic operations when possible (like [`File::create_new`] instead of checking existence then creating).
//! - Keep file open for the duration of operations.

mod core;

use std::{
    fmt,
    fs::{File, FileTimes, Metadata, OpenOptions, Permissions},
    io::{self, IoSlice, IoSliceMut, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::core::compression::compress;

#[derive(Debug)]
pub struct ChronoFile {
    path: PathBuf,
    inner: File,
    chrono: File,
}

// TODO: move to util
fn construct_chrono_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut chrono_path = path.as_ref().to_owned();
    chrono_path.set_extension("chrono");
    chrono_path
}

// TODO: impl buffered
// TODO: impl lock
impl ChronoFile {
    /// Attempts to open a chronologically versioned File in read-only mode.
    ///
    /// This method defers to [std::fs::File] open method, for further information look there.
    ///
    /// # Errors
    ///
    /// This function will return an error if `path` does not already exist.
    /// Other errors may also be returned according to [`OpenOptions::open`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::io::Read;
    ///
    /// use chronofile::ChronoFile;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let mut f = ChronoFile::open("foo.txt")?;
    ///     let mut data = vec![];
    ///     f.read_to_end(&mut data)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<ChronoFile> {
        let inner = OpenOptions::new().read(true).open(path.as_ref())?;
        let chrono = OpenOptions::new()
            .read(true)
            .open(construct_chrono_path(&path))?;
        Ok(ChronoFile { path: path.as_ref().to_owned(), inner, chrono })
    }

    /// Opens a chronologically versioned file in write-only mode.
    ///
    /// This function will create a file if it does not exist,
    /// and will truncate it if it does. If the file already exists a chrono version is created
    /// saving the state of the file prior to truncation.
    ///
    /// This method defers to [std::fs::File] create method, for further information look there.
    ///
    /// Depending on the platform, this function may fail if the
    /// full directory path does not exist.
    /// See the [`OpenOptions::open`] function for more details.
    ///
    /// See also [`std::fs::write()`][self::write] for a simple function to
    /// create a file with some given data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::io::Write;
    ///
    /// use chronofile::ChronoFile;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let mut f = ChronoFile::create("foo.txt")?;
    ///     f.write_all(&1234_u32.to_be_bytes())?;
    ///     Ok(())
    /// }
    /// ```
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<ChronoFile> {
        let inner = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.as_ref())?;
        let chrono = OpenOptions::new()
            .append(true)
            .create(true)
            .open(construct_chrono_path(&path))?;
        Ok(ChronoFile { path: path.as_ref().to_owned(), inner, chrono })
    }

    /// Creates a new file in read-write mode; error if the file exists.
    ///
    /// This function will create a file if it does not exist, or return an error if it does. This
    /// way, if the call succeeds, the file returned is guaranteed to be new.
    /// If a file exists at the target location, creating a new file will fail with [`AlreadyExists`]
    /// or another error based on the situation. See [`OpenOptions::open`] for a
    /// non-exhaustive list of likely errors.
    ///
    /// This option is useful because it is atomic. Otherwise between checking whether a file
    /// exists and creating a new one, the file may have been created by another process (a [TOCTOU]
    /// race condition / attack).
    ///
    /// [`AlreadyExists`]: crate::io::ErrorKind::AlreadyExists
    /// [TOCTOU]: self#time-of-check-to-time-of-use-toctou
    ///
    /// This method defers to [std::fs::File] create_new method, for further information look there.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::io::Write;
    ///
    /// use chronofile::ChronoFile;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let mut f = ChronoFile::create_new("foo.txt")?;
    ///     f.write_all("Hello, world!".as_bytes())?;
    ///     Ok(())
    /// }
    /// ```
    pub fn create_new<P: AsRef<Path>>(path: P) -> io::Result<ChronoFile> {
        let inner = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path.as_ref())?;
        let chrono = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(construct_chrono_path(&path))?;
        Ok(ChronoFile { path: path.as_ref().to_owned(), inner, chrono })
    }

    // Attempts to open a chronologically versioned File in
    /// Queries metadata about the underlying file.
    ///
    /// This method defers to [std::fs::File] metadata method, for further information look there.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use chronofile::ChronoFile;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let mut f = ChronoFile::open("foo.txt")?;
    ///     let metadata = f.metadata()?;
    ///     Ok(())
    /// }
    /// ```
    pub fn metadata(&self) -> io::Result<Metadata> {
        self.inner.metadata()
    }

    /// Changes the permissions on the underlying file.
    ///
    /// Permissions changes do not result in a new diff as the file contents has not changed.
    ///
    /// This method defers to [std::fs::File] set_permissions method, for further information look there.
    ///
    /// # Platform-specific behavior
    ///
    /// This function currently corresponds to the `fchmod` function on Unix and
    /// the `SetFileInformationByHandle` function on Windows. Note that, this
    /// [may change in the future][changes].
    ///
    /// [changes]: io#platform-specific-behavior
    ///
    /// # Errors
    ///
    /// This function will return an error if the user lacks permission change
    /// attributes on the underlying file. It may also return an error in other
    /// os-specific unspecified cases.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// fn main() -> std::io::Result<()> {
    ///     use chronofile::ChronoFile;
    ///
    ///     let file = ChronoFile::open("foo.txt")?;
    ///     let mut perms = file.metadata()?.permissions();
    ///     perms.set_readonly(true);
    ///     file.set_permissions(perms)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Note that this method alters the permissions of the underlying file,
    /// even though it takes `&self` rather than `&mut self`.
    pub fn set_permissions(&self, perm: Permissions) -> io::Result<()> {
        self.inner.set_permissions(perm)
    }

    /// Changes the timestamps of the underlying file.
    ///
    /// Timestamp changes do not result in a new diff as the file contents has not changed.
    ///
    /// This method defers to [std::fs::File] set_times method, for further information look there.
    ///
    /// # Platform-specific behavior
    ///
    /// This function currently corresponds to the `futimens` function on Unix (falling back to
    /// `futimes` on macOS before 10.13) and the `SetFileTime` function on Windows. Note that this
    /// [may change in the future][changes].
    ///
    /// On most platforms, including UNIX and Windows platforms, this function can also change the
    /// timestamps of a directory. To get a `File` representing a directory in order to call
    /// `set_times`, open the directory with `File::open` without attempting to obtain write
    /// permission.
    ///
    /// [changes]: io#platform-specific-behavior
    ///
    /// # Errors
    ///
    /// This function will return an error if the user lacks permission to change timestamps on the
    /// underlying file. It may also return an error in other os-specific unspecified cases.
    ///
    /// This function may return an error if the operating system lacks support to change one or
    /// more of the timestamps set in the `FileTimes` structure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// fn main() -> std::io::Result<()> {
    ///     use std::fs::{self, FileTimes};
    ///
    ///     use chronofile::ChronoFile;
    ///
    ///     let src = fs::metadata("src")?;
    ///     let dest = ChronoFile::open("dest")?;
    ///     let times = FileTimes::new()
    ///         .set_accessed(src.accessed()?)
    ///         .set_modified(src.modified()?);
    ///     dest.set_times(times)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn set_times(&self, times: FileTimes) -> io::Result<()> {
        self.inner.set_times(times)
    }

    /// Changes the modification time of the underlying file.
    ///
    /// Timestamp changes do not result in a new diff as the file contents has not changed.
    ///
    /// This is an alias for `set_times(FileTimes::new().set_modified(time))`.
    pub fn set_modified(&self, time: SystemTime) -> io::Result<()> {
        self.set_times(FileTimes::new().set_modified(time))
    }
}

// TODO: Writing
// - Prepare the file data (the version you want to store).
// - Compute the uncompressed checksum (e.g., SHA-256) of the file data.
// - Compress the file data (e.g., with zstd).
// - Compute the compressed checksum (e.g., SHA-256) of the compressed data.
// - Write the version block to the file in this order:
//     - F (file marker)
//     - Length of the compressed data (as ASCII)
//     - :
//     - Timestamp (e.g., 2026-02-26T12:00:00Z)
//     - :SHA256_COMPRESSED:
//     - Compressed checksum (hex)
//     - :SHA256_UNCOMPRESSED:
//     - Uncompressed checksum (hex)
//     - Compressed data
//  - write the actual file, rollback the version update if error
impl Write for ChronoFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // write file
        let written_bytes = self.inner.write(buf)?;

        // Chrono
        let system_time = SystemTime::now();
        let datetime: DateTime<Utc> = system_time.into();

        // calc file checksum
        let uncompressed_hash = Sha256::digest(buf);
        // compress file. stream read the file contents and compress to a buffer
        // here we have to open the file again as read so we can compress the data
        self.inner = File::open(&self.path)?;
        let (compressed_data, compressed_len) = compress(&self.inner)?;
        // calc compressed checksum
        let compressed_hash = Sha256::digest(&compressed_data);
        // construct diff
        let diff = format!(
            "F{}:{:?}:SHA256_COMPRESSED:{:x}:SHA256_UNCOMPRESSED:{:x}:{}",
            compressed_len,
            datetime,
            compressed_hash,
            uncompressed_hash,
            hex::encode(compressed_data)
        );
        // save to diff file if exists
        let _chrono_bytes = self.chrono.write(diff.as_bytes())?;

        Ok(written_bytes)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.inner.write_vectored(bufs)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Read for ChronoFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (self).inner.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.inner.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.inner.read_to_end(buf)
    }
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.inner.read_to_string(buf)
    }
}

pub trait Restore {
    fn restore(&mut self) -> io::Result<()>;
}

// TODO: restore
//
// version format (note multiple per file):
//  - F - file marker - start of a new update
//  - length of data - the length of the compressed data
//  - : - seperator
//  - timestamp - the timestamp the version was created
//  - : - seperator
//  - SHA256_COMPRESSED - identifier
//  - : - seperator
//  - compressed checksum
//  - SHA256_UNCOMPRESED - identifie
//  - : - seperator
//  - uncompressed checkesum
//  - compressed data (length defined above)
//
//
// - Read the file marker (F).
// - Read the length of the compressed data (up to the next :).
// - Read the timestamp (up to the next :).
// - Read the compressed checksum (after :SHA256_COMPRESSED:).
// - Read the uncompressed checksum (after :SHA256_UNCOMPRESSED:).
// - Read the compressed data (using the length from step 2).
// - Verify the compressed checksum:
//     - Compute SHA-256 of the compressed data.
//     - Compare with the stored compressed checksum.
//     - If they don’t match, the file is corrupted.
// - Decompress the data.
// - Verify the uncompressed checksum:
//     - Compute SHA-256 of the decompressed data.
//     - Compare with the stored uncompressed checksum.
//     - If they don’t match, decompression failed or the file is corrupted.
// - Return the decompressed data (now verified as correct).
// - Save decompressed data as File
impl Restore for ChronoFile {
    fn restore(&mut self) -> io::Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod test_utils {
    use tempfile::TempDir;

    pub const MB: usize = 1_048_576;

    pub fn create_temp_dir(prefix: &str) -> TempDir {
        TempDir::with_prefix(prefix).unwrap()
    }
}

#[cfg(test)]
mod chronofile_tests {
    use super::test_utils::{create_temp_dir, MB};
    use super::*;

    #[test]
    fn create() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");

        let file = ChronoFile::create(file_path);
        assert!(file.is_ok());
    }

    #[test]
    fn create_already_exists() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");

        {
            let file = ChronoFile::create_new(&file_path);
            assert!(file.is_ok());
            let mut file = file.unwrap();
            let content = vec![0_u8; MB];
            let bytes = file.write(&content).unwrap();
            assert!(bytes > 0);
        }

        for i in 0..=2 {
            let mut file = ChronoFile::create(&file_path).unwrap();
            let content = vec![i as u8; MB];
            let bytes = file.write(&content).unwrap();
            assert!(bytes > 0);
        }

        // TODO: assert that there are two file markers in the .chrono file

        // assert the contents are the last written data
        let mut file = ChronoFile::open(&file_path).unwrap();
        let mut buf = Vec::new();
        let _bytes = file.read(&mut buf).unwrap();
        dbg!(buf);
        // assert_eq!(buf, vec![2; MB])
    }

    #[test]
    fn create_new() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");

        let file = ChronoFile::create_new(file_path);
        assert!(file.is_ok());
    }

    #[test]
    fn create_new_fails_if_exists() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");
        {
            let _file = File::create(&file_path).unwrap();
        }

        let file = ChronoFile::create_new(file_path);
        assert!(file.is_err());
    }

    #[test]
    fn open_readable() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");

        {
            let _ = ChronoFile::create_new(&file_path);
        }
        let file_stamp = ChronoFile::open(file_path);
        assert!(file_stamp.is_ok());
    }

    #[test]
    fn set_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("perms-test.txt");

        {
            let _ = ChronoFile::create_new(&file_path);
        }

        let file_stamp = ChronoFile::open(&file_path).unwrap();
        let mut perms = file_stamp.metadata().unwrap().permissions();
        let old_mode = perms.mode();
        perms.set_mode(old_mode | 0o400); // Add read for owner

        assert!(file_stamp.set_permissions(perms).is_ok());

        let new_perms = file_stamp.metadata().unwrap().permissions();
        assert_eq!(new_perms.mode() & 0o400, 0o400);
    }

    #[test]
    fn set_times() {
        use std::fs::FileTimes;
        use std::time::{SystemTime, UNIX_EPOCH};

        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("times-test.txt");

        {
            let _ = ChronoFile::create_new(&file_path);
        }

        let file_stamp = ChronoFile::open(&file_path).unwrap();
        let now = SystemTime::now();
        let times = FileTimes::new().set_accessed(now).set_modified(now);

        assert!(file_stamp.set_times(times).is_ok());

        let metadata = file_stamp.metadata().unwrap();
        // Allow some slack for filesystem precision
        assert!(
            metadata
                .accessed()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                >= now.duration_since(UNIX_EPOCH).unwrap().as_secs() - 1
        );
        assert!(
            metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                >= now.duration_since(UNIX_EPOCH).unwrap().as_secs() - 1
        );
    }

    #[test]
    fn set_modified() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("modified-test.txt");

        {
            let _ = ChronoFile::create_new(&file_path);
        }

        let file_stamp = ChronoFile::open(&file_path).unwrap();
        let now = SystemTime::now();

        assert!(file_stamp.set_modified(now).is_ok());

        let metadata = file_stamp.metadata().unwrap();
        // Allow some slack for filesystem precision
        assert!(
            metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                >= now.duration_since(UNIX_EPOCH).unwrap().as_secs() - 1
        );
    }
}

#[cfg(test)]
mod read_tests {

    use std::io::Write;

    use super::test_utils::create_temp_dir;
    use super::*;

    #[test]
    fn test_read() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("read-test.txt");
        // Write some test data
        let test_data = b"hello world";
        {
            let mut file = ChronoFile::create_new(&file_path).unwrap();
            let _ = file.write(test_data).unwrap();
        }

        // Open with ChronoFile and read
        let mut cf = ChronoFile::open(&file_path).unwrap();
        let mut buf = vec![0; test_data.len()];
        let n = cf.read(&mut buf).unwrap();
        assert_eq!(n, test_data.len());
        assert_eq!(&buf, test_data);
    }

    #[test]
    fn test_read_vectored() {
        use std::io::IoSliceMut;

        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("read_vectored-test.txt");

        // Write some test data
        let test_data = b"hello world";
        {
            let mut file = ChronoFile::create_new(&file_path).unwrap();
            let _ = file.write(test_data).unwrap();
        }

        // Open with ChronoFile and read with scatter/gather
        let mut cf = ChronoFile::open(&file_path).unwrap();
        let mut buf1 = vec![0; 5];
        let mut buf2 = vec![0; 7];
        let mut bufs = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
        let n = cf.read_vectored(&mut bufs).unwrap();

        assert_eq!(n, test_data.len());
        assert_eq!(&buf1[..5], b"hello");
        assert_eq!(&buf2[..6], b" world"); // Only compare the bytes that were written
    }

    #[test]
    fn test_read_to_end() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("read_to_end-test.txt");

        // Write some test data
        let test_data = b"hello world";
        {
            let mut file = ChronoFile::create_new(&file_path).unwrap();
            let _ = file.write(test_data).unwrap();
        }

        // Open with ChronoFile and read to end
        let mut cf = ChronoFile::open(&file_path).unwrap();
        let mut buf = Vec::new();
        let n = cf.read_to_end(&mut buf).unwrap();

        assert_eq!(n, test_data.len());
        assert_eq!(&buf, test_data);
    }

    #[test]
    fn test_read_to_string() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("read_to_string-test.txt");

        // Write some test data
        let test_data = b"hello world";
        {
            let mut file = ChronoFile::create_new(&file_path).unwrap();
            let _ = file.write(test_data).unwrap();
        }

        // Open with ChronoFile and read to string
        let mut cf = ChronoFile::open(&file_path).unwrap();
        let mut buf = String::new();
        let n = cf.read_to_string(&mut buf).unwrap();

        assert_eq!(n, test_data.len());
        assert_eq!(&buf.into_bytes(), test_data);
    }
}

#[cfg(test)]
mod write_tests {

    use std::io::Write;

    use super::test_utils::create_temp_dir;
    use super::*;

    #[test]
    fn test_write() {
        let dir = create_temp_dir("ChronoFileWrite");
        let mut file_path = dir.keep();
        file_path.push("write-test.txt");

        // Write some test data
        let content = vec![0u8; 1_048_576]; // 1MB of zeros;
        let mut cf = ChronoFile::create_new(&file_path).unwrap();
        let bytes = cf.write(&content).unwrap();
        assert!(bytes > 0);
    }
}
