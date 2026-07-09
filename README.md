# ChronoFile

A time-travelable file for Rust. `ChronoFile` is a drop-in for
[`std::fs::File`] — its `Read` and `Write` impls pass straight through to the
underlying file — while storing versioned diffs in a companion `.chrono` file so
you can restore previous versions.

## When To Use It

`ChronoFile` fits when a single file changes over time and you want its history
without standing up a database or a full VCS:

- **Local-first apps** — game saves, editor documents, or design tools that need
  undo/redo and "restore to an earlier save" across sessions.
- **Config and state files** — keep a rollback trail for a settings or state
  file so a bad change can be reverted.
- **Append-mostly data with checkpoints** — journals, logs, or notes where you
  periodically `commit()` a version you might want back.
- **Audit trails** — retain every committed revision of a document for later
  inspection, stored compactly as diffs rather than full copies.

Because it drops in for [`std::fs::File`], existing code that reads and writes a
file keeps working — you add `commit()` at the points that mark a version.

### When *not* to use it

- **Many files / directory trees** — this versions one file; use Git or a
  backup tool.
- **Concurrent writers** — no locking yet (see roadmap); assumes a single
  writer.
- **Huge files with frequent commits** — the whole log is currently rewritten
  per commit and replayed on `open`; snapshotting/indexing are on the roadmap.
- **Binary blobs that change wholesale** — diffs save little when each version
  shares nothing with the last.

## How It Works

- **Main file** (`file.dat`) — always holds the current bytes. Reads and writes
  go straight to it, exactly like `std::fs::File`.
- **Chrono file** (`file.dat.chrono`) — a compact binary log (bincode-encoded)
  of per-version diffs computed with [`diffy`].

A version is created only when you call [`commit`](#versions-are-explicit). Each
commit diffs the current contents against the previous commit and appends the
patch to the `.chrono` log. Restoring replays the patches from the start up to
the target version, rewrites the main file with those contents, and records the
restore as a new version (so the restore itself is part of the history and
becomes the baseline for the next `commit`).

## Versions Are Explicit

**Writing does not create a version — you must call `commit()`.**

`Write::write` is a low-level byte sink called many times per logical operation
(`write_all`, `writeln!`, `io::copy`), so tying a version to it would mint bogus,
half-written versions. Instead: write freely, then `commit()` when the current
contents are worth keeping. A commit with no changes since the last one is a
no-op.

## Usage

Create a file, write to it, and commit versions:

```rust
use std::io::Write;
use chronofile::{ChronoFile, History};

let mut cf = ChronoFile::create("save.dat")?;

writeln!(cf, "level 1 complete")?;
let v0 = cf.commit()?;             // Some(0)

writeln!(cf, "level 2 complete")?;
let v1 = cf.commit()?;             // Some(1)

cf.commit()?;                      // None — nothing changed
```

Read the current contents like any file:

```rust
use std::io::Read;

let mut cf = ChronoFile::open("save.dat")?;
let mut buf = String::new();
cf.read_to_string(&mut buf)?;
```

Restore an earlier version and count versions:

```rust
use chronofile::History;

let n = cf.list_versions()?;       // number of committed versions
let bytes = cf.restore(0)?;        // rewrite the file to version 0, return its bytes
```

`restore` overwrites the main file with the target version's contents and
records the restore as a new version. Restoring the current latest version
changes nothing, so it records no new version.

### Opening semantics

Unlike [`std::fs::File::open`] (read-only), `ChronoFile::open` opens the main
file for **read + write** and creates the `.chrono` companion if missing — a
`ChronoFile` must be able to record new versions. The main file must already
exist; `create` makes a new one (truncating any existing main and `.chrono`
files).

## Status

Implemented:

- `ChronoFile::create` / `ChronoFile::open`
- `Read` / `Write` (pass-through to the main file)
- `commit()` — record a version, returns the version id (`None` if unchanged)
- `History::restore` — replay the log to reconstruct a version, rewrite the
  file, and record the restore as a new version
- `History::list_versions` — number of committed versions

## Roadmap

- Version metadata for `list_versions()` (timestamps, tags) rather than a bare
  count.
- `Seek` impl for full `std::fs::File` parity.
- `sync_all` / `metadata` and other `File` parity methods.
- Integrity checksums to detect corruption on read.
- Periodic full snapshots to cut replay time on long histories.
- Indexing for fast random access to diffs.
- Optional compression (`zstd`, `lz4`) for diffs.
- Pluggable diff algorithms (`bsdiff`, `xdelta3`, rolling hash).
- Streaming API for large files.
- Concurrency support via file locks.
- Optional encryption for diffs.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

[`std::fs::File`]: https://doc.rust-lang.org/std/fs/struct.File.html
[`std::fs::File::open`]: https://doc.rust-lang.org/std/fs/struct.File.html#method.open
[`diffy`]: https://crates.io/crates/diffy
