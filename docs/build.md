# Build and verification

## Requirements

- Rust/Cargo 1.81 or later. Use rustfmt and Clippy for the full check command.
- For native examples: a C99 compiler, a C++17 compiler, and Linux development libraries.
- No Rust dependencies are downloaded for the library itself. CI toolchain/actions setup requires network access.

Run from the repository root:

```sh
cargo build --workspace --release
cargo run --example inspect
cargo run --example legacy
sh scripts/check.sh
```

The check script runs formatting checks, Clippy with warnings denied, workspace tests and rustdoc examples, core-only compilation, API documentation with warnings denied, both Rust-guide examples, both Rust integration examples, a release native build, and both native examples. Native examples compile with `-DNDEBUG`; their explicit behavioral checks remain active. The script fixes `CARGO_TARGET_DIR` to the repository's `target/` so all native paths agree. Set `CC` and `CXX` to alternate compiler executable paths if required.

The full `scripts/check.sh` smoke-link commands target Linux. The separate `scripts/release.py package --target TARGET` command builds a native package and compiles/runs consumers against its extracted header/libraries on Linux, Windows/MSVC, or macOS. Cross-platform CI configuration is not proof of a completed remote run; see [release validation limits](releases.md#validation-boundary).

## Rust consumers

For a sibling checkout, add:

```toml
[dependencies]
libspd = { path = "../libspd" }
```

Use `libspd::{Image, ImageMut, Read, Write, Timing, TimingField}`. The root crate has no standard-library or allocator requirement. Implement `Read` and optionally `Write` in the application, fill a caller-owned snapshot, and pass the snapshot to the image API. Parsing and editing do not trigger adapter calls. See `examples/inspect.rs` for a complete synthetic example and [the field matrix](fields.md) for typed decoding/editing support.

See the [Rust API guide](rust.md) for borrowed image/metadata lifetimes, exact timing edits, error handling, explicit integrity policy, and application-owned adapter examples. Public API documentation and intra-doc links are compiler-checked.

## C and C++ consumers on Linux

Build `cargo build -p libspd-ffi --release`. Include `libspd.h` from `include/`; ship that header together with the matching ABI version's library.

Static C linking:

```sh
mkdir -p target/native
cc -std=c99 -Wall -Wextra -Werror -pedantic -Iinclude \
  examples/native.c target/release/libspd.a -ldl -lpthread -lm \
  -o target/native/native-c
target/native/native-c
```

Shared C++ linking:

```sh
c++ -std=c++17 -Wall -Wextra -Werror -pedantic -Iinclude \
  examples/native.cpp -Ltarget/release -lspd \
  '-Wl,-rpath,$ORIGIN/../release' -o target/native/native-cpp
target/native/native-cpp
```

`$ORIGIN` is intentionally passed literally to the Linux linker/loader, not expanded by the shell. These examples keep shared artifacts under `target/release`; deployment should use the application's normal installation and loader policy. Static system-library requirements depend on the Rust target; the command above is for the checked Linux host.

Both examples use the same C ABI and synthetic in-memory callbacks. They exercise DDR4/DDR5 timing and manufacturing fields, exact edits, rejected-edit atomicity, explicit CRC repair, and unsupported-layout errors. They validate observable behavior rather than contacting a DIMM. See [native safety and callback contracts](design.md#native-abi-version-1) before calling from an application.

## Automation and evidence limits

GitHub Actions is the primary automation host: `.github/workflows/ci.yml` runs the Linux quality/MSRV checks, and the shared native workflow builds and consumes ZIP packages on Linux, Windows, and macOS. `.github/workflows/release.yml` prepares draft releases from matching version tags; manual dispatch builds artifacts only. See [native releases](releases.md) for targets, archive layout, trigger/permission policy, and verification limits. No remote workflow execution or publication is implied by local checks.

Synthetic tests can establish bounds handling, byte preservation, CRC arithmetic, and ABI behavior. They cannot establish JEDEC semantic conformance, supported real-module coverage, hardware compatibility, write safety, or protection restoration. See the fixture and specification gates in [the plan](plan.md).

## Foundation verification record

On 2026-10-25, `sh scripts/check.sh` passed on Linux x86_64 with Rust 1.99.0,
and the complete script also passed under Rust 1.81.0. Native examples used
GCC/G++ 15.3.0.

- All 13 regression tests passed (7 core and 6 native-boundary tests).
- Formatting, Clippy with warnings denied, core compilation, and API documentation
  with warnings denied passed on both Rust versions.
- The Rust application-owned memory adapter ran successfully. The C example
  linked the release static library; the C++17 example linked and loaded the
  release shared library. Both exercised explicit CRC repair and synchronous
  callbacks, including partial-failure reporting.

Only synthetic image data was used. Remote CI and non-Linux targets were not
exercised.

## Typed-field verification record

The complete `sh scripts/check.sh` passed on Linux x86_64 with Rust 1.99.0
and Rust 1.81.0 after the selected-field implementation:

- All 48 regression tests passed: 7 raw/core, 6 module-integrity, 7 manufacturing,
  17 timing, and 11 native-boundary tests.
- Formatting, Clippy with warnings denied, core-only compilation, and API
  documentation with warnings denied passed on both toolchains.
- The Rust memory-adapter example decoded and edited timing/manufacturing data,
  rejected a fractional-nanosecond edit atomically, and explicitly repaired CRC.
- C99 static-link and C++17 shared-link programs ran with `-DNDEBUG` and active
  checks for DDR4/DDR5 typed access, precise units/nCK, mutation/error paths,
  independent CRC domains, and application-owned callbacks.

Deterministic tests cover every truncation below each supported field’s required
end, invalid encodings, exact representability limits, unknown layouts, and
unchanged-byte preservation. Inputs remain synthetic, except for tiny numerical
CRC vectors independently checked against public vendor tables. No full vendor
dump is distributed, and no real-module, remote-CI, non-Linux, or completed
fuzzing validation is claimed.

## Legacy verification record

The full check script passed with Rust 1.99.0 and 1.81.0 on Linux x86_64 after
adding legacy codecs. All 72 tests passed, including legacy/DDR3 truncation,
identity offsets, timing units and signed fractions, adjacent-nibble preservation,
atomic failures, raw metadata widths, additive checksums, and both DDR3 CRC spans.

`examples/legacy.rs` ran actual decode/edit/metadata/integrity paths for FPM,
EDO, SDRAM, DDR, DDR2, and DDR3. The C99 static and C++17 shared examples ran
the same generation matrix with `NDEBUG` and active checks. The existing modern
example and tests also passed. Formatting, Clippy, core compilation, and API
documentation passed on both toolchains.

See [legacy support](legacy.md) for exact revision/field limits and source
qualifications. Synthetic verification does not establish real-module coverage,
hardware compatibility, or full JEDEC conformance.

## Publication metadata

The public repository is [TeskesLab/libspd](https://github.com/TeskesLab/libspd).
Both Cargo manifests declare repository, homepage, documentation links, and
registry categories/keywords. The FFI dependency specifies both a local path and
version `0.0.2`; registry publication requires publishing the core crate first.
The core package was locally packaged and compiled with
`cargo package -p libspd --allow-dirty`. This does not publish anything or verify
crate-name availability. Native consumers still obtain `include/libspd.h` from
the matching public repository revision; the FFI crate archive does not bundle
that workspace-level header.

## Rust documentation coverage

Both crates deny `missing_docs` and `rustdoc::broken_intra_doc_links`, covering
their public types, variants, fields, constants, methods, and adapter contracts.
The Rust guide is tested with `rustdoc --test` by `scripts/check.sh`; its source
examples cannot silently drift away from the compiled API.

Verification passed with Rust 1.81.0 and 1.99.0: 72 regression tests, six API
rustdoc examples, two Rust-guide examples, strict documentation builds, Clippy,
formatting, and the Rust/C/C++ integration runs. Documentation/lint changes did
not alter runtime behavior or native ABI signatures.
