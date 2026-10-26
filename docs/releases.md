# Native binary releases

GitHub is the primary CI/release host for the public [TeskesLab/libspd](https://github.com/TeskesLab/libspd) repository. Workflows build native libraries and ZIP packages; they do not publish Rust crates to crates.io or provide hardware drivers.

## Build targets

Each matrix job builds on its native OS and compiles/runs C and C++ consumers against an **unpacked ZIP**, not just the build directory.

| Rust target / archive suffix | GitHub runner | Shared library | Other libraries |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Ubuntu 22.04 | `libspd.so` | `libspd.a` |
| `x86_64-pc-windows-msvc` | Windows 2022, MSVC x64 | `spd.dll` | `spd.dll.lib` import library; `spd.lib` static library |
| `x86_64-apple-darwin` | macOS 15 Intel | `libspd.dylib` | `libspd.a` |
| `aarch64-apple-darwin` | macOS 15 ARM64 | `libspd.dylib` | `libspd.a` |

These are selected build/CI targets, not a promise of compatibility with every release of each OS. Linux builds use the Ubuntu runner's glibc/toolchain; no musl or universal-old-glibc binary is implied. Windows is the MSVC ABI, not MinGW. macOS Intel and ARM64 archives are separate, not a universal binary. Platform runtime requirements remain those of the selected Rust/linker environment.

**Windows naming:** the FFI crate's library name is `spd`, so Cargo produces `spd.dll`. Keep it beside its matching import library. Renaming only the DLL to `libspd.dll` breaks the import library's embedded runtime name; the release flow preserves Cargo's names deliberately.

## Package contents

Each asset is named `libspd-VERSION-TARGET.zip`, with a matching `.zip.sha256` sidecar. The ZIP contains one `libspd-VERSION-TARGET/` directory with:

- `include/libspd.h`: the ABI-matched C99/C++17 header.
- `lib/`: target-native shared/static libraries, plus the Windows import library.
- `LICENSE`, `README.md`, and the linked documentation under `docs/`.
- `examples/native.c` and `examples/native.cpp`: synthetic consumer examples.
- `release.json`: package version, Rust target, ABI version, added rustc flags, and toolchain-reported static-link dependencies.

The package is for native consumers; Rust applications normally depend on the `libspd` crate/source instead of loading the C ABI. No programmer executable, firmware, credentials, or real-module SPD dump is included. A checksum detects accidental download changes; a checksum sidecar alone is not a cryptographic provenance/signature guarantee.

## CI and release triggers

- `ci.yml`: push/pull-request checks, Linux MSRV 1.81 and stable verification, and the native ZIP build matrix.
- `native.yml`: shared matrix implementation called by CI and the release workflow, avoiding separate packaging implementations.
- `release.yml`: version-tag pushes and manual dispatch. Manual dispatch builds downloadable workflow artifacts only; it does not create a release.
- A matching `vVERSION` tag creates a **draft** GitHub release after all native packaging/consumer jobs and both Linux quality/MSRV jobs succeed. No tag is created by the workflow. The root and FFI package versions must match, and the tag must match that exact version.
- Release write permission is limited to the draft-release job in the canonical GitHub repository. Build jobs use read-only repository access. No release API is invoked from untrusted pull-request builds.

Draft creation is still a remote publication action. This task only adds configuration: it does not push a tag, invoke a workflow, create a release, or publish a crate. Maintainers must explicitly authorize those actions when performing a release. A draft lets a maintainer inspect assets/notes before separately making it public. Existing releases are not silently overwritten; resolve an existing draft deliberately before rerunning a failed publication job.

The old `.gitea/workflows/ci.yml` is replaced rather than kept as a second active pipeline. Some Gitea/Forgejo configurations recognize GitHub workflow layouts, but hosted-runner labels, reusable workflows, `GITHUB_TOKEN`, `gh`, and artifact actions are GitHub-specific integrations here. Do not assume this release workflow is portable to Gitea without adapting those services.

## Local packaging and consumption

Requires Python 3.11+, Cargo/Rust stable, and native C/C++ development tools for the host platform. Windows also needs an initialized MSVC developer environment. Run from a checkout:

```sh
python scripts/release.py version
python scripts/release.py version --tag v0.1.0
python scripts/release.py package --target x86_64-unknown-linux-gnu
```

Use the matching target from the table on Windows/macOS. Packaging rejects foreign targets rather than cross-building an artifact it cannot execute locally. It uses the repository `target/` directory to avoid disagreement with custom Cargo target paths. Build artifacts and ZIPs stay under `target/`, outside version control.

The package command builds the FFI crate, collects exact required files, writes the archive/checksum, extracts into a temporary directory, compiles the included C/C++ programs using the extracted header/libraries, and runs them. Consumer failures make the command fail; no hardware test is implied.

For a downloaded package, verify its SHA-256 using your platform's checksum tool, unpack it, add its `include/` to your include paths and its `lib/` to linker paths. See [the FFI guide](ffi.md) for ownership and function contracts, and [build instructions](build.md) for source-checkout examples. Shared-library deployments must make the matching library discoverable to the OS loader. Static linking can require target-specific Rust system libraries; inspect linker diagnostics rather than copying Linux link flags to Windows/macOS.

## Validation boundary

Local Linux execution can prove the Linux package and consumer path only. Windows and macOS paths require their actual native matrix jobs; configuration alone is not evidence those jobs have run. The workflow must be pushed to GitHub and executed before claiming remote release validation. None of the native smoke programs establishes DIMM compatibility or permission to program real hardware.

## Platform documentation

- [GitHub workflow permissions](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions): `contents: write` permits release creation; build jobs do not need it.
- [Passing artifacts between jobs](https://docs.github.com/en/actions/tutorials/store-and-share-data): release jobs wait for matrix builds and download their uploaded ZIP assets.
