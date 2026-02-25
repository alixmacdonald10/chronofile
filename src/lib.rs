use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

pub struct ChronoFile {
    inner: File,
}

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
    /// and will truncate it if it does.
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
        // TODO: if exists it truncates it so handle this with the chrono

        Ok(ChronoFile {
            inner: OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path.as_ref())?,
        })
    }
}

impl From<File> for ChronoFile {
    fn from(value: File) -> Self {
        ChronoFile {
            inner: value
        }
    }
}

#[cfg(test)]
mod tests {
    use tempdir::TempDir;

    use super::*;

    fn create_temp_dir(path: &str) -> TempDir {
        TempDir::new(path).unwrap()
    }

    #[test]
    fn create() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.into_path();
        file_path.push("create-test.txt");

        let file = ChronoFile::create(file_path);
        assert!(file.is_ok());
    }

    #[test]
    fn open() {
        let dir = create_temp_dir("ChronoFile");
        let mut file_path = dir.into_path();
        file_path.push("create-test.txt");

        {
            let _file = File::create(&file_path).unwrap();
        }
        let file_stamp = ChronoFile::open(file_path);
        assert!(file_stamp.is_ok());
    }
}
