# `ChronoFile`

A time-travelable file for Rust. `ChronoFile` is a drop-in for
[`std::fs::File`] — its `Read`, `Write` and `Seek` impls pass straight through
to the main file — paired with a compressed companion `.chrono` file that holds
a full snapshot of every committed version.

Each snapshot carries a UUID v7 id, so ids sort by commit time, and a crc32
checksum that is verified before any preview or restore. Nothing is versioned automatically: a version exists only after you call
`commit()`.

## Usage

```rust,no_run
use std::io::Write;
use std::time::SystemTime;

use chronofile::{ChronoFile, History};

fn main() -> std::io::Result<()> {
    let mut cf = ChronoFile::create("save.dat")?;

    write!(cf, "level 1")?;
    let v1 = cf.commit()?.expect("file changed");

    let checkpoint = SystemTime::now();

    write!(cf, " -> level 2")?;
    cf.commit()?;

    // Unchanged contents record nothing.
    assert_eq!(cf.commit()?, None);

    for snapshot in cf.list_snapshots()? {
        println!("{}", snapshot.id);
    }

    // Read an old version without touching the main file.
    assert_eq!(cf.preview(v1)?, Some(b"level 1".to_vec()));

    // Roll the main file back. The restore is recorded as a new version.
    cf.restore(v1)?;

    // Or pick the latest version committed at or before a point in time.
    cf.restore_at(checkpoint)?;
    Ok(())
}
```

Version ids are [`uuid::Uuid`]. Add the `uuid` crate to your own
`Cargo.toml` if you need to store or build ids yourself.

## API

`std::fs::File` parity:

- `ChronoFile::create` / `ChronoFile::open`
- `Read` / `Write` / `Seek` (pass-through to the main file)
- `metadata` / `chrono_metadata` — filesystem metadata for the main and
  `.chrono` files
- `set_permissions` — change the main file's permissions
- `sync_all` / `sync_data` — flush both files to disk (`.chrono` first)
- `set_len` — truncate/extend the main file

Versioning (`History` trait):

- `commit()` — snapshot the main file. Returns `Some(id)`, or `None` if the
  contents match the last commit.
- `list_snapshots()` — every snapshot, oldest first.
- `preview(id)` / `preview_at(time)` — read a version's bytes without
  changing the main file. Returns `None` if no version exists at or before
  the id/time.
- `restore(id)` / `restore_at(time)` — overwrite the main file with a version
  and record the result as a new version. Fails with `InvalidInput` if no
  version exists at or before the id/time.

## Limitations

- Snapshot history larger than 256 MiB uncompressed cannot be loaded.
- Each commit reads and rewrites the whole `.chrono` file.
- Single writer only. There is no file locking.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/alixmacdonald10/chronofile/blob/main/LICENSE-APACHE))
- MIT License ([LICENSE-MIT](https://github.com/alixmacdonald10/chronofile/blob/main/LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

[`std::fs::File`]: https://doc.rust-lang.org/std/fs/struct.File.html
[`uuid::Uuid`]: https://docs.rs/uuid/latest/uuid/struct.Uuid.html
