# libspd

Hardware-independent **Serial Presence Detect (SPD)** image parsing and editing
for Rust, C, and C++.

Use a dump file, an in-memory snapshot, or bytes acquired by your own hardware
adapter. libspd decodes and edits the image; your application controls device
access and programming policy.

## What it provides

- An allocation-free, `no_std` Rust core with borrowed image views and no unsafe code.
- Selected timing and manufacturing fields for FPM/EDO, SDRAM, and DDR through DDR5.
- Exact timing edits, bounded part-number/serial edits, and explicit checksum/CRC repair.
- A C ABI with caller-owned buffers, documented headers, and optional synchronous I/O callbacks.

Unknown bytes are preserved. Failed image edits change nothing. Parsing does not
require a valid checksum, and editing never repairs integrity data implicitly.

**Support is field- and revision-specific, not complete JEDEC conformance.**
See the [DDR4/DDR5 matrix](docs/fields.md) and [legacy matrix](docs/legacy.md).
There are no hardware drivers, automatic flashing, vendor-profile codecs, or
configuration-level semantic validation.

## Rust quick start

Requires Rust **1.81+**. For a local checkout (crates.io setup is pending; see the
[publishing guide](docs/releases.md#cratesio-publishing)):

```toml
[dependencies]
libspd = { path = "../libspd" }
```

```rust
use libspd::{Error, Image, ImageMut, TimingField};

fn main() -> Result<(), Error> {
    // Synthetic partial DDR5 image, not a valid complete module description.
    let mut bytes = [0u8; 512];
    bytes[..4].copy_from_slice(&[0x30, 0x10, 0x12, 0x02]);
    bytes[20..22].copy_from_slice(&625u16.to_le_bytes());

    let image = Image::new(&bytes);
    println!("{:?}", image.identity()?);
    let mut timing = image.timing(TimingField::TckMin)?;
    timing.picoseconds = 624;

    let mut edited = ImageMut::new(&mut bytes);
    edited.set_timing(TimingField::TckMin, timing)?;
    edited.repair_base_crc()?; // Explicit choice, not a setter side effect.
    assert!(edited.as_image().base_crc()?.is_valid());
    Ok(())
}
```

See the [Rust guide](docs/rust.md) for ownership, errors, metadata, and external
adapters. Generate the API reference with `cargo doc -p libspd --no-deps --open`.

## C and C++

Build with `cargo build -p libspd-ffi --release`, include
[`libspd.h`](include/libspd.h), and link the native library. Linux builds produce
`target/release/libspd.a` and `libspd.so`. C++ uses the same C header directly.

The [native release flow](docs/releases.md) prepares draft releases on both
[Gitea](https://gitnet.teske.zip/TeskesLabOrg/libspd/releases) and
[GitHub](https://github.com/TeskesLab/libspd/releases). Both provide Linux x64
and cross-compiled Linux ARM64 and Windows x64/MinGW ZIPs plus Debian 12 amd64
and arm64 `libspd1` runtime and `libspd-dev` development packages. GitHub
additionally provides native Windows x64/MSVC and macOS x64/ARM64 ZIPs.
ARM64 C/C++ consumers run under QEMU and MinGW consumers under Wine during
packaging; neither substitutes for native platform/hardware validation.
GitHub uses reusable packaging workflows with a four-target native matrix;
Gitea uses a direct Debian-container job and local composite actions shared
by CI and release. Including SHA-256 sidecars, Gitea's draft has 14 assets and
GitHub's has 20.
No APT repository, package signing, or automatic public release is provided.
Only the Rust `libspd` crate has a separate, manually approved
[crates.io publishing workflow](docs/releases.md#cratesio-publishing).

The [FFI guide](docs/ffi.md) includes complete examples, linking commands,
ownership rules, and callback contracts. Applications supply hardware access;
libspd never opens a bus or invokes callbacks while parsing an image.

## Build and verify

```sh
cargo build --workspace --release
cargo run --example inspect
cargo run --example legacy
sh scripts/check.sh
```

The full check command requires Linux, C99/C++17 compilers, rustfmt, and Clippy.
It runs regression tests, executable documentation, and linked native examples.
Examples use synthetic data, not hardware validation.

- [Build, verification, and publication notes](docs/build.md)
- [Architecture and integration boundaries](docs/design.md)
- [Development plan and evidence gates](docs/plan.md)

Licensed under [Apache-2.0](LICENSE).
