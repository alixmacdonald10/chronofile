# ChronoFile 

**Chronofile** is a simple Rust crate that provides chronologically versioned files. A **Copy-On-Write (COW)** mechanism is used to safely append data to files while preserving previous versions. With each modification, it creates a new copy of the file, ensuring your original data remains intact.

### Features:
- Append data to files without modifying the original.
- Create new versions with each change, enabling easy rollback.
- Simple and lightweight, perfect for backup systems and version control.

### Usage:




---

TODO:
- impl From<File> for ChronoFile
- implement trait which defines the time travel,restore etc
- create backend for backuped file
- create struct for backuped file
- use diffs not just new files
- benches vs file etc
- impl file methods
- pipeline for review of rust file methods if change then open issue
