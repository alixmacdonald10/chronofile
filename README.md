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
patch to the `.chrono` log, tagged with a CRC32 checksum of the full contents at
that version. Both `preview` and `restore` reconstruct a version by replaying
patches from the start up to the target — `preview` returns those bytes
read-only (the working file is untouched), while `restore` also rewrites the
main file and records the restore as a new version (so the restore itself is
part of the history and becomes the baseline for the next `commit`).

As each patch is replayed, the reconstructed contents are checksummed against
the value recorded at commit time; a mismatch means the `.chrono` log has been
corrupted and the operation fails with an
[`InvalidData`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html#variant.InvalidData)
error rather than returning silently wrong bytes. See
[Integrity and recovery](#integrity-and-recovery).

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

List versions, peek at one without disturbing the file, then restore:

```rust
use chronofile::History;

// each version carries its id and commit time so you can tell them apart
for v in cf.list_versions()? {
    println!("version {} committed at {:?}", v.id, v.timestamp);
}

// look at what an old version contains WITHOUT changing the working file
let old = cf.preview(0)?;           // bytes as of version 0; file untouched

// once you've picked one, restore it (rewrites the file + records a new version)
let bytes = cf.restore(0)?;
```

`list_versions` returns a `VersionInfo { id, timestamp }` per committed version.
Use `preview(id)` to compare candidates read-only, then `restore(id)` to apply
the one you want. `restore` overwrites the main file and records the restore as
a new version; restoring the current latest version changes nothing, so it
records none.

You can also select by **time** instead of id — `preview_at(t)` and
`restore_at(t)` resolve to the latest version committed at or before `t`:

```rust
use std::time::{Duration, SystemTime};

let an_hour_ago = SystemTime::now() - Duration::from_secs(3600);
let bytes = cf.restore_at(an_hour_ago)?;   // "roll back to how it was an hour ago"
```

### Opening semantics

Unlike [`std::fs::File::open`] (read-only), `ChronoFile::open` opens the main
file for **read + write** and creates the `.chrono` companion if missing — a
`ChronoFile` must be able to record new versions. The main file must already
exist; `create` makes a new one (truncating any existing main and `.chrono`
files).

### Integrity and recovery

Every committed version stores a CRC32 checksum of its full contents. Because
`open`, `preview`, and `restore` all replay the log, a corrupted `.chrono` file
is caught the moment its bytes no longer reconstruct the recorded checksum —
those calls return an `InvalidData` error instead of handing back wrong data.

**Your current data is not lost when this happens.** The main file (`file.dat`)
always holds the live bytes independently of the log, so it stays readable even
when the history cannot be replayed:

- **Read the current contents directly.** Open `file.dat` with a plain
  [`std::fs::File`] — it is untouched by the corruption and holds the latest
  committed (and any uncommitted) bytes.
- **Reset the history.** Delete the companion `.chrono` file. The next
  `ChronoFile::open` recreates an empty log, so the file is usable again and new
  commits start a fresh history. The old version history is gone, but the
  current file contents are preserved.

Do **not** reach for `ChronoFile::create` to recover: it truncates the *main*
file as well as the `.chrono` log, discarding your current data. There is no
in-library repair or partial-salvage API yet (see roadmap).

## Status

Implemented:

- `ChronoFile::create` / `ChronoFile::open`
- `Read` / `Write` / `Seek` (pass-through to the main file)
- `metadata` / `chrono_metadata` — filesystem metadata for the main and
  `.chrono` files
- `sync_all` / `sync_data` — flush both files to disk (`.chrono` first)
- `set_len` — truncate/extend the main file (recorded on next `commit`)
- `commit()` — record a version, returns the version id (`None` if unchanged)
- `History::list_versions` — every version with its id and commit timestamp
- `History::preview` / `preview_at` — reconstruct a version's contents without
  touching the main file (by id, or as of a point in time)
- `History::restore` / `restore_at` — replay the log to reconstruct a version
  (by id or as of a time), rewrite the file, and record the restore as a new
  version
- Integrity checksums — a per-version CRC32 verified on replay; a corrupt
  `.chrono` log fails with `InvalidData` instead of returning wrong data (see
  [Integrity and recovery](#integrity-and-recovery))

## Roadmap

- Richer version metadata (messages, tags) beyond id + timestamp.
- `set_permissions`, `try_clone` and other `File` parity methods.
- History repair/recovery API — salvage or truncate a corrupt log in-library
  (drop bad tail entries, rebuild from the last valid version) instead of
  deleting the `.chrono` file by hand.
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
