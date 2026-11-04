# ChronoFile

A time-travelable file for Rust. `ChronoFile` is a drop-in for
[`std::fs::File`] — its `Read`, `Write` and `Seek` impls pass straight through
to the underlying file — paired with a companion `.chrono` file that will hold
compressed full-version snapshots.

> **Work in progress:** versioning is being rewritten. Currently only the
> `std::fs::File` parity surface is implemented.

## Status

Implemented:

- `ChronoFile::create` / `ChronoFile::open`
- `Read` / `Write` / `Seek` (pass-through to the main file)
- `metadata` / `chrono_metadata` — filesystem metadata for the main and
  `.chrono` files
- `set_permissions` — change the main file's permissions
- `sync_all` / `sync_data` — flush both files to disk (`.chrono` first)
- `set_len` — truncate/extend the main file

Stubbed (API defined, implementation pending):

- `commit()` — record a version as a compressed full snapshot
- `History::list_versions` / `preview` / `preview_at` / `restore` /
  `restore_at`

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
