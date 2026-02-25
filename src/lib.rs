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

use std::{
    fmt,
    fs::{File, FileTimes, Metadata, OpenOptions, Permissions},
    io::{self, IoSliceMut, Read},
    path::Path,
    time::SystemTime,
};

pub struct ChronoFile {
    inner: File,
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
        // TODO: Check if chronoversioned else create that

        Ok(ChronoFile {
            inner: OpenOptions::new().read(true).open(path.as_ref())?,
        })
    }

    /// Opens a chronologically versioned file in write-only mode.
    ///
    /// This function will create a file if it does not exist,
    /// and will truncate it if it does. The truncation results in a new
    /// diff being created.
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
        // TODO: if exists it truncates it but make a chrono backup
        Ok(ChronoFile {
            inner: OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path.as_ref())?,
        })
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
        Ok(ChronoFile {
            inner: OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(path.as_ref())?,
        })
    }

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

impl From<File> for ChronoFile {
    fn from(value: File) -> Self {
        ChronoFile { inner: value }
    }
}

impl fmt::Debug for ChronoFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
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

#[cfg(test)]
mod test_utils {
    use tempfile::TempDir;

    pub fn create_temp_dir(prefix: &str) -> TempDir {
        TempDir::with_prefix(prefix).unwrap()
    }
}

#[cfg(test)]
mod chronofile_tests {
    use super::test_utils::create_temp_dir;
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
    fn open() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.keep();
        file_path.push("create-test.txt");

        {
            let _file = File::create(&file_path).unwrap();
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
            let _file = File::create(&file_path).unwrap();
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
            let _file = File::create(&file_path).unwrap();
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
            let _file = File::create(&file_path).unwrap();
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
            let mut file = File::create(&file_path).unwrap();
            file.write_all(test_data).unwrap();
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
            let mut file = File::create(&file_path).unwrap();
            file.write_all(test_data).unwrap();
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
            let mut file = File::create(&file_path).unwrap();
            file.write_all(test_data).unwrap();
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
        let test_data = "hello world";
        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(test_data.as_bytes()).unwrap();
        }

        // Open with ChronoFile and read to string
        let mut cf = ChronoFile::open(&file_path).unwrap();
        let mut buf = String::new();
        let n = cf.read_to_string(&mut buf).unwrap();

        assert_eq!(n, test_data.len());
        assert_eq!(&buf, test_data);
    }
}
