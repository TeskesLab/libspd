# Native binary releases

Version-tag workflows prepare maintainer-reviewed **draft** releases on both [Gitea](https://gitnet.teske.zip/TeskesLabOrg/libspd/releases) and [GitHub](https://github.com/TeskesLab/libspd/releases). Both forges build Linux and cross-compiled Windows GNU ZIPs plus Debian runtime/development packages. GitHub also builds native Windows MSVC and both macOS architectures. These workflows do not publish Rust crates to crates.io or provide hardware drivers.

## Build targets

Native jobs build on their target OS and compile/run C and C++ consumers against an **unpacked ZIP**, not just the build directory. The Windows GNU job instead cross-compiles on Linux and runs extracted Windows consumers under Wine.

| Rust target / archive suffix | GitHub build environment | Gitea build environment | Shared library | Other libraries |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Ubuntu 22.04 | Debian 12 container | `libspd.so` | `libspd.a` |
| `x86_64-pc-windows-gnu` | Debian 12 container, MinGW/Wine | Debian 12 container, MinGW/Wine | `spd.dll` | `libspd.dll.a` import library; `libspd.a` static library |
| `x86_64-pc-windows-msvc` | Windows 2022, MSVC x64 | Not built | `spd.dll` | `spd.dll.lib` import library; `spd.lib` static library |
| `x86_64-apple-darwin` | macOS 15 Intel | Not built | `libspd.dylib` | `libspd.a` |
| `aarch64-apple-darwin` | macOS 15 ARM64 | Not built | `libspd.dylib` | `libspd.a` |

These are selected build/CI targets, not a promise of compatibility with every OS release. The GitHub Linux ZIP retains the Ubuntu 22.04 glibc/toolchain baseline; the Gitea Linux ZIP uses Debian 12's baseline. The same Linux asset name therefore does **not** imply identical binaries across forges. No musl or universal-old-glibc binary is implied. macOS Intel and ARM64 archives are separate, not a universal binary. Platform runtime requirements remain those of the selected Rust/linker environment.

**Windows toolchain choice:** the GNU ZIP uses MinGW import/static libraries for GNU-toolchain consumers and avoids needing a Windows build worker. It is separate from the MSVC ZIP, which is built and executed on a native Windows runner and is intended for MSVC consumers; do not interchange their static/import libraries. Wine verifies the cross-built C/C++ examples, not every real Windows version, Windows loader behavior, or MSVC compatibility.

**Windows naming:** the FFI crate's library name is `spd`, so Cargo produces `spd.dll`. Keep it beside its matching import library. Renaming only the DLL to `libspd.dll` breaks the import library's embedded runtime name; the release flow preserves Cargo's names deliberately.

## Package contents

Each asset is named `libspd-VERSION-TARGET.zip`, with a matching `.zip.sha256` sidecar. The ZIP contains one `libspd-VERSION-TARGET/` directory with:

- `include/libspd.h`: the ABI-matched C99/C++17 header.
- `lib/`: target-native shared/static libraries, plus the Windows import library.
- `LICENSE`, `README.md`, and the linked documentation under `docs/`.
- `examples/native.c` and `examples/native.cpp`: synthetic consumer examples.
- `release.json`: package version, Rust target, ABI version, added rustc flags, and toolchain-reported static-link dependencies.

The package is for native consumers; Rust applications normally depend on the `libspd` crate/source instead of loading the C ABI. No programmer executable, firmware, credentials, or real-module SPD dump is included. A checksum detects accidental download changes; a checksum sidecar alone is not a cryptographic provenance/signature guarantee.

## Debian 12 amd64 packages

Debian packages build natively in the Debian 12-based `node:22-bookworm-slim` image on both forges: in a reusable GitHub job and in Gitea's direct packaging job. Node is required for JavaScript actions on Gitea, whose runner does not inject it into custom containers. The package baseline is **Debian 12 (Bookworm), amd64**; it is not a promise of compatibility with older Debian releases, other distributions, or other architectures. ZIP packaging remains separate.

Each release adds these assets and their individual `.deb.sha256` sidecars:

- `libspd1_DEBIANVERSION_amd64.deb`: runtime shared library with ABI SONAME `libspd.so.1`.
- `libspd-dev_DEBIANVERSION_amd64.deb`: `libspd.h`, static library, unversioned shared-library linker link, and `libspd.pc`; depends on the exact matching `libspd1` version.

The runtime package name and SONAME track C ABI version 1. An incompatible ABI change requires a new ABI version, SONAME, and runtime package name together.

Packages use the `/usr` prefix: the header is `/usr/include/libspd.h`, libraries are in `/usr/lib/x86_64-linux-gnu/`, and pkg-config metadata is `/usr/lib/x86_64-linux-gnu/pkgconfig/libspd.pc`. Use `pkg-config --cflags --libs libspd` for shared consumers. `pkg-config --cflags --libs --static libspd` also supplies the static library's system dependencies, but does not force static linking: link `/usr/lib/x86_64-linux-gnu/libspd.a` explicitly, as the installed smoke check does.

Cargo SemVer is validated against both manifests before conversion: `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]` becomes `MAJOR.MINOR.PATCH[~PRERELEASE][+BUILD]-1`. For example, `1.2.3` maps to `1.2.3-1`, and `1.2.3-rc.1+build.7` maps to `1.2.3~rc.1+build.7-1`. The Debian revision is fixed at `1`; the Git tag remains the original Cargo version with its `v` prefix.

Verify the downloaded sidecars, then install both matching local files (replace the example version):

```sh
sha256sum -c libspd1_0.0.2-1_amd64.deb.sha256
sha256sum -c libspd-dev_0.0.2-1_amd64.deb.sha256
sudo apt-get install --no-install-recommends ./libspd1_0.0.2-1_amd64.deb ./libspd-dev_0.0.2-1_amd64.deb
```

For local packaging in a Debian 12 amd64 checkout, install `ca-certificates curl git build-essential python3 dpkg-dev pkg-config binutils xz-utils` and Rust stable, then run:

```sh
python3 scripts/debian.py version
python3 scripts/debian.py package
# Install the produced files from target/dist/ with apt-get as above.
python3 scripts/debian.py smoke
```

Packaging writes both `.deb` files and sidecars to `target/dist/` and tests consumers against extracted packages. Both the reusable GitHub job and Gitea's composite packaging action additionally install both packages with `apt-get --no-install-recommends`, then check installed pkg-config metadata, headers, static/shared C and C++ consumers, and runtime loader discovery without loader-path overrides. The installed smoke command requires the matching packages and a source checkout: it uses the checkout's synthetic examples against installed headers/libraries, because minimal Debian images can omit documentation files during installation.

These are downloadable binary packages, **not an APT repository**: no repository metadata, package signing, automatic public release, or automatic installation is provided. SHA-256 sidecars detect corruption, not publisher authenticity. Release automation stops at a maintainer-reviewed draft on the corresponding forge.


## CI and release triggers

- `ci.yml`: push/pull-request Linux MSRV 1.81 and stable quality checks, version validation, native ZIPs, Windows GNU cross-packaging, and Debian packaging on both forges.
- On GitHub, both `ci.yml` and `release.yml` call the reusable packaging workflows below. Their package jobs are guarded to run only on GitHub.
  - `native.yml`: static four-target matrix on Ubuntu 22.04, Windows 2022/MSVC, macOS 15 Intel, and macOS 15 ARM64, using Python 3.11 and native C/C++ consumer execution.
  - `mingw.yml`: Debian 12 amd64 cross-build using Rust stable's `x86_64-pc-windows-gnu` target, MinGW C/C++ compilers, and Wine; uploads the ZIP and sidecar as `libspd-mingw-windows-gnu`.
  - `debian.yml`: Debian 12 amd64 build, extracted-consumer tests, apt installation, and installed-consumer tests; uploads the two packages and sidecars as `libspd-debian-amd64`.
- On Gitea, both parent workflows use a direct top-level packaging job in `node:22-bookworm-slim`, not a nested reusable workflow. After installing Git/TLS support and checking out the source without persisted credentials, the job calls the local composite action `.github/actions/gitea-packages/action.yml` with the validated version. This action shares the Linux ZIP, Debian package/install-smoke, and MinGW/Wine packaging steps between CI and release, using system Python 3.11 and Rust stable. It uploads all eight package/checksum files together as `libspd-gitea-packages`.
- `release.yml`: version-tag pushes and manual dispatch on both forges. Manual dispatch builds downloadable workflow artifacts only; it does not create a release.
- A matching `vVERSION` tag creates a **draft** release on the corresponding forge after all its packaging/consumer jobs and both Linux quality/MSRV jobs succeed. No tag is created by the workflow. The root and FFI package versions must match, and the tag must match that exact version.
- Release write permission is limited to the draft-release job in the canonical repositories (`TeskesLab/libspd` on GitHub and `TeskesLabOrg/libspd` on Gitea). Build jobs use read-only repository access. No release API is invoked from untrusted pull-request builds. The privileged job does not execute downloaded code: it validates the exact expected package names and checksums before attaching an explicit asset list.

### Exact release asset sets

For version `0.0.2`, **both** forges attach these four package assets:

- `libspd-0.0.2-x86_64-unknown-linux-gnu.zip`
- `libspd-0.0.2-x86_64-pc-windows-gnu.zip`
- `libspd1_0.0.2-1_amd64.deb`
- `libspd-dev_0.0.2-1_amd64.deb`

GitHub additionally attaches these three package assets:

- `libspd-0.0.2-x86_64-pc-windows-msvc.zip`
- `libspd-0.0.2-x86_64-apple-darwin.zip`
- `libspd-0.0.2-aarch64-apple-darwin.zip`

Every package asset has its own same-name `.sha256` sidecar. Gitea therefore has **two ZIPs, two Debian packages, and four checksum files (8 assets)**. GitHub has **five ZIPs, two Debian packages, and seven checksum files (14 assets)**.

Draft creation is a remote publication action; a maintainer must authorize pushing the version tag and triggering release preparation, then inspect assets/notes before separately making the draft public. Existing releases are not silently overwritten; resolve an existing draft deliberately before rerunning a failed publication job.

Gitea 1.27 recognizes the `.github/workflows` layout, so no duplicate `.gitea/workflows` pipeline is needed. GitHub uses the official `actions/upload-artifact@v4` and `actions/download-artifact@v4`; Gitea uses the compatible [`ChristopherHX/gitea-upload-artifact@v4`](https://github.com/ChristopherHX/gitea-upload-artifact) and [`ChristopherHX/gitea-download-artifact@v4`](https://github.com/ChristopherHX/gitea-download-artifact) forks. The Gitea runner must resolve action references through GitHub and use the public external Gitea URL, as the upload fork documents. Its artifact client uses the runner-provided `ACTIONS_RESULTS_URL` for RPC; uploaded/downloaded signed URLs must also be reachable from job containers. Setting only `GITHUB_SERVER_URL` does not fix artifact endpoint reachability.

## Local packaging and consumption

Requires Python 3.11+, Cargo/Rust stable, and native C/C++ development tools for the host platform. Windows also needs an initialized MSVC developer environment. Run from a checkout:

```sh
python scripts/release.py version
python scripts/release.py version --tag v0.0.2
python scripts/release.py package --target x86_64-unknown-linux-gnu
```

Use the matching native target from the table on Windows/macOS. `scripts/release.py` rejects foreign targets rather than cross-building an artifact it cannot execute locally. It uses the repository `target/` directory to avoid disagreement with custom Cargo target paths. Build artifacts and ZIPs stay under `target/`, outside version control.

The native package command builds the FFI crate, collects exact required files, writes the archive/checksum, extracts into a temporary directory, compiles the included C/C++ programs using the extracted header/libraries, and runs them. Consumer failures make the command fail; no hardware test is implied.

### Windows GNU cross-packaging

In a Debian 12 amd64 environment, install `ca-certificates curl git build-essential python3 dpkg-dev pkg-config binutils xz-utils gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64 wine wine64` and Rust stable, then run:

```sh
rustup target add x86_64-pc-windows-gnu
python3 scripts/mingw.py package
```

The separate cross-package command writes `target/dist/libspd-0.0.2-x86_64-pc-windows-gnu.zip` and its `.sha256` sidecar (using the validated manifest version). It extracts the archive, compiles Windows C and C++ consumers against the extracted headers and shared/static libraries, and executes those consumers with Wine. A successful Linux cross-build alone is not sufficient: consumer execution failures fail packaging.

For a downloaded package, verify its SHA-256 using your platform's checksum tool, unpack it, add its `include/` to your include paths and its `lib/` to linker paths. See [the FFI guide](ffi.md) for ownership and function contracts, and [build instructions](build.md) for source-checkout examples. Shared-library deployments must make the matching library discoverable to the OS loader. Static linking can require target-specific Rust system libraries; inspect linker diagnostics rather than copying Linux link flags to Windows/macOS.

## Validation boundary

Local execution proves only the package and consumer paths actually exercised. Debian installed-package validation requires the Debian 12 amd64 environment; Windows GNU smoke checks require MinGW and Wine; native Windows MSVC and macOS paths require their actual native matrix jobs. Configuration alone is not evidence those jobs have run. Workflows must be pushed to the respective forge and executed before claiming remote release validation. None of the smoke programs establishes DIMM compatibility or permission to program real hardware.

## Platform documentation

- [GitHub workflow permissions](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions): `contents: write` permits release creation; build jobs do not need it.
- [Passing artifacts between jobs](https://docs.github.com/en/actions/tutorials/store-and-share-data): release jobs wait for all ZIP/Debian builds and download their uploaded assets.
- [Gitea workflow permissions](https://docs.gitea.com/1.27/usage/actions/token-permissions/): Gitea 1.27 maps `contents: write` to code and release write access.
