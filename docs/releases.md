# Releases and crates.io publishing

Version-tag workflows prepare maintainer-reviewed **draft** releases on both [Gitea](https://gitnet.teske.zip/TeskesLabOrg/libspd/releases) and [GitHub](https://github.com/TeskesLab/libspd/releases). Both forges build Linux x64, cross-compiled Linux ARM64, and cross-compiled Windows GNU ZIPs plus Debian amd64/arm64 runtime/development packages. GitHub also builds native Windows MSVC and both macOS architectures. These workflows do not publish Rust crates to crates.io or provide hardware drivers.

Only the Rust API crate, `libspd`, is intended for crates.io publication. The `libspd-ffi` crate has `publish = false`; C/C++ users consume the native release packages or build it from this repository.

## crates.io publishing

Only `libspd` is published to crates.io: it is the portable Rust source crate, not a native ZIP, Debian package, shared library, or hardware driver. `libspd-ffi` remains `publish = false`. Native packages still follow the separate maintainer-reviewed draft-release flow below; publishing a crate neither publishes those drafts nor uploads their assets.

Before authorizing any upload, a maintainer must verify these prerequisites:

1. Check that the `libspd` name is available on crates.io, or that the publishing account has ownership of the existing crate. Confirm the intended owners before publishing.
2. Consult the current [crates.io trusted-publishing documentation](https://crates.io/docs/trusted-publishing), especially whether first-crate/bootstrap publishing is supported. Do not assume OIDC can create a new crate. If bootstrap requires another method, arrange a separately authorized initial publication using the current official procedure; never place credentials in this repository.
3. Configure the crates.io trusted publisher with the exact GitHub owner `TeskesLab`, repository `libspd`, workflow filename `publish.yml`, and environment `crates-io`.
4. Create the GitHub `crates-io` environment with a nonempty required-human-reviewer list. Self-approval is currently allowed for single-maintainer testing; **prevent self-review** can be enabled later without changing the workflow. Until then, approval does not provide independent review. Select custom deployment branch/tag policies with exactly one policy: tag `v*`, and no branch policies. The workflow checks required reviewers and these tag restrictions through the GitHub API and fails closed if they are missing or unreadable. Ensure these protections are available and enforced for the repository's GitHub plan. YAML alone does not configure these settings. See [GitHub deployment environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).

Keep the root and FFI package versions and the FFI dependency synchronized (currently `0.0.3`), including the workspace lockfile. Crates.io versions are immutable: a published version cannot be replaced with different contents, even after yanking. Fix a published crate by releasing a new version, not by moving its tag or retrying different contents under the same version.

From the clean source checkout for the intended version, the exact local crate dry run is:

```sh
cargo publish --dry-run -p libspd --locked
```

For an authorized crates.io upload, manually dispatch `.github/workflows/publish.yml` on the matching `vVERSION` tag, for example `v0.0.3`; do not dispatch on a branch. The workflow validates the tag against both package versions, then verifies the source crate before the protected publishing job requests approval through `crates-io`. Review the verification result and tagged source before approving. Only that protected job receives `id-token: write`; `rust-lang/crates-io-auth-action@v1` exchanges GitHub OIDC for a short-lived token and revokes it in its post-step. The upload command is exactly:

```sh
cargo publish --locked -p libspd --registry crates-io
```

The token is supplied to Cargo through the job environment, never committed or pasted into workflow files. `publish.yml` is GitHub-only: Gitea does not upload crates, obtain crates.io credentials, or bypass GitHub approval. Tag pushes and the existing native `release.yml` workflow do not automatically upload to crates.io. No remote settings, tag creation, workflow dispatch, or uploads are implied by preparing these files.

## Build targets

Native jobs build on their target OS and compile/run C and C++ consumers against an **unpacked ZIP**, not just the build directory. The Windows GNU job instead cross-compiles on Linux and runs extracted Windows consumers under Wine. The Linux ARM64 job cross-compiles with the GNU AArch64 toolchain in Debian 12 and executes extracted C static and C++ shared consumers explicitly through `qemu-aarch64`.

| Rust target / archive suffix | GitHub build environment | Gitea build environment | Shared library | Other libraries |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | Ubuntu 22.04 | Debian 12 container | `libspd.so` | `libspd.a` |
| `aarch64-unknown-linux-gnu` | Debian 12 container, GNU cross-tools/QEMU | Debian 12 container, GNU cross-tools/QEMU | `libspd.so` | `libspd.a` |
| `x86_64-pc-windows-gnu` | Debian 12 container, MinGW/Wine | Debian 12 container, MinGW/Wine | `spd.dll` | `libspd.dll.a` import library; `libspd.a` static library |
| `x86_64-pc-windows-msvc` | Windows 2022, MSVC x64 | Not built | `spd.dll` | `spd.dll.lib` import library; `spd.lib` static library |
| `x86_64-apple-darwin` | macOS 15 Intel | Not built | `libspd.dylib` | `libspd.a` |
| `aarch64-apple-darwin` | macOS 15 ARM64 | Not built | `libspd.dylib` | `libspd.a` |

These are selected build/CI targets, not a promise of compatibility with every OS release. The GitHub x64 Linux ZIP retains the Ubuntu 22.04 glibc/toolchain baseline; the Gitea x64 Linux ZIP uses Debian 12's baseline. The same x64 Linux asset name therefore does **not** imply identical binaries across forges. Linux ARM64 ZIPs use the Debian 12 GNU/glibc baseline on both forges. No musl or universal-old-glibc binary is implied. macOS Intel and ARM64 archives are separate, not a universal binary. Platform runtime requirements remain those of the selected Rust/linker environment.

**Windows toolchain choice:** the GNU ZIP uses MinGW import/static libraries for GNU-toolchain consumers and avoids needing a Windows build worker. It is separate from the MSVC ZIP, which is built and executed on a native Windows runner and is intended for MSVC consumers; do not interchange their static/import libraries. Wine verifies the cross-built C/C++ examples, not every real Windows version, Windows loader behavior, or MSVC compatibility.

**Windows naming:** the FFI crate's library name is `spd`, so Cargo produces `spd.dll`. Keep it beside its matching import library. Renaming only the DLL to `libspd.dll` breaks the import library's embedded runtime name; the release flow preserves Cargo's names deliberately.

**Linux ARM64 validation:** ARM64 builds use Rust stable's `aarch64-unknown-linux-gnu` target, `aarch64-linux-gnu-gcc`/`g++`, and Debian multiarch libraries. QEMU user-mode executes the cross-built consumers; it is not native ARM64 hardware, a full ARM64 OS boot, or hardware/SPD validation. No host `binfmt_misc` registration or privileged emulator setup is required.

## Package contents

Each asset is named `libspd-VERSION-TARGET.zip`, with a matching `.zip.sha256` sidecar. The ZIP contains one `libspd-VERSION-TARGET/` directory with:

- `include/libspd.h`: the ABI-matched C99/C++17 header.
- `lib/`: target-native shared/static libraries, plus the Windows import library.
- `LICENSE`, `README.md`, and the linked documentation under `docs/`.
- `examples/native.c` and `examples/native.cpp`: synthetic consumer examples.
- `release.json`: package version, Rust target, ABI version, added rustc flags, and toolchain-reported static-link dependencies.

The package is for native consumers; Rust applications normally depend on the `libspd` crate/source instead of loading the C ABI. No programmer executable, firmware, credentials, or real-module SPD dump is included. A checksum detects accidental download changes; a checksum sidecar alone is not a cryptographic provenance/signature guarantee.

## Debian 12 amd64 and arm64 packages

Debian packages build in the Debian 12-based `node:22-bookworm-slim` image on both forges: amd64 builds natively, while arm64 uses GNU cross-compilers and QEMU. GitHub uses reusable jobs; Gitea uses a direct packaging job and shared local composite actions. Node is required for JavaScript actions on Gitea, whose runner does not inject it into custom containers. The package baseline is **Debian 12 (Bookworm), amd64 or arm64**; it is not a promise of compatibility with older Debian releases, other distributions, or other architectures. ZIP packaging remains separate.

Each release adds these assets and their individual `.deb.sha256` sidecars:

- `libspd1_DEBIANVERSION_ARCH.deb`: runtime shared library with ABI SONAME `libspd.so.1`.
- `libspd-dev_DEBIANVERSION_ARCH.deb`: `libspd.h`, static library, unversioned shared-library linker link, and `libspd.pc`; depends on the exact matching `libspd1` version. `ARCH` is `amd64` or `arm64`.

The runtime package name and SONAME track C ABI version 1. An incompatible ABI change requires a new ABI version, SONAME, and runtime package name together.

Packages use the `/usr` prefix: the header is `/usr/include/libspd.h`, libraries are in `/usr/lib/x86_64-linux-gnu/` (amd64) or `/usr/lib/aarch64-linux-gnu/` (arm64), and each library directory contains `pkgconfig/libspd.pc`. Both package architectures are multiarch-coinstallable at the same version. Use `pkg-config --cflags --libs libspd` for native shared consumers. For cross-compilation, select ARM64 metadata with `PKG_CONFIG_LIBDIR=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig`. `pkg-config --cflags --libs --static libspd` also supplies the static library's system dependencies, but does not force static linking: link the architecture's `libspd.a` explicitly, as the installed smoke check does.

Cargo SemVer is validated against both manifests before conversion: `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]` becomes `MAJOR.MINOR.PATCH[~PRERELEASE][+BUILD]-1`. For example, `1.2.3` maps to `1.2.3-1`, and `1.2.3-rc.1+build.7` maps to `1.2.3~rc.1+build.7-1`. The Debian revision is fixed at `1`; the Git tag remains the original Cargo version with its `v` prefix.

Debian changelog dates use `SOURCE_DATE_EPOCH` when set, otherwise the source Git commit's Unix timestamp. Builds from source archives without Git metadata must set `SOURCE_DATE_EPOCH` to that timestamp. Use the same source and timestamp for both architectures: common headers and documentation, including the compressed changelog, must be byte-identical for multiarch coinstallation.

Verify the downloaded sidecars, then install both matching local files (replace the example version):

```sh
sha256sum -c libspd1_0.0.3-1_amd64.deb.sha256
sha256sum -c libspd-dev_0.0.3-1_amd64.deb.sha256
sudo apt-get install --no-install-recommends ./libspd1_0.0.3-1_amd64.deb ./libspd-dev_0.0.3-1_amd64.deb
```

For native local packaging in a Debian 12 amd64 checkout, install `ca-certificates curl git build-essential python3 dpkg-dev pkg-config binutils xz-utils` and Rust stable, then run:

```sh
python3 scripts/debian.py version
python3 scripts/debian.py package
# Install the produced files from target/dist/ with apt-get as above.
python3 scripts/debian.py smoke
```

Packaging writes both `.deb` files and sidecars to `target/dist/` and tests consumers against extracted packages. Both forges additionally install both packages of each built architecture with `apt-get --no-install-recommends`, then check installed pkg-config metadata, headers, static/shared C and C++ consumers, and runtime loader discovery without loader-path overrides. ARM64 consumers execute explicitly under `qemu-aarch64 -L /` using the installed foreign libraries; amd64 consumers run natively. Gitea installs arm64 alongside its already installed amd64 packages, exercising multiarch coinstallation. The installed smoke command requires the matching packages and a source checkout: it uses the checkout's synthetic examples against installed headers/libraries, because minimal Debian images can omit documentation files during installation. The default architecture for `package` and `smoke` remains `amd64`; `version` has no architecture option.

These are downloadable binary packages, **not an APT repository**: no repository metadata, package signing, automatic public release, or automatic installation is provided. SHA-256 sidecars detect corruption, not publisher authenticity. Release automation stops at a maintainer-reviewed draft on the corresponding forge.


## CI and release triggers

- `ci.yml`: push/pull-request Linux MSRV 1.81 and stable quality checks, version validation, native ZIPs, Windows GNU and Linux ARM64 cross-packaging, and Debian packaging on both forges.
- On GitHub, both `ci.yml` and `release.yml` call the reusable packaging workflows below. Their package jobs are guarded to run only on GitHub.
  - `native.yml`: static four-target matrix on Ubuntu 22.04, Windows 2022/MSVC, macOS 15 Intel, and macOS 15 ARM64, using Python 3.11 and native C/C++ consumer execution.
  - `mingw.yml`: Debian 12 amd64 cross-build using Rust stable's `x86_64-pc-windows-gnu` target, MinGW C/C++ compilers, and Wine; uploads the ZIP and sidecar as `libspd-mingw-windows-gnu`.
  - `debian.yml`: Debian 12 amd64 build, extracted-consumer tests, apt installation, and installed-consumer tests; uploads the two packages and sidecars as `libspd-debian-amd64`.
  - `arm64.yml`: Debian 12 amd64-to-arm64 GNU cross-build, using `.github/actions/arm64-packages/action.yml` for ZIP/Debian packaging, extracted QEMU consumers, apt installation, and installed QEMU consumers. Uploads the ARM64 ZIP, both arm64 Debian packages, and their sidecars as the six-file `libspd-arm64-packages` artifact.
- On Gitea, both parent workflows use a direct top-level packaging job in `node:22-bookworm-slim`, not a nested reusable workflow. After installing Git/TLS support and checking out the source without persisted credentials, the job calls the local composite action `.github/actions/gitea-packages/action.yml` with the validated version. This action shares the Linux x64 ZIP, amd64 Debian package/install-smoke, and MinGW/Wine packaging steps between CI and release, using system Python 3.11 and Rust stable. It retains the eight-file `libspd-gitea-packages` artifact and also invokes the same ARM64 composite used by GitHub to produce the separate six-file `libspd-arm64-packages` artifact. Publishers merge `libspd-*` artifacts and attach only their exact expected names.
- `release.yml`: version-tag pushes and manual dispatch on both forges. Manual dispatch builds downloadable workflow artifacts only; it does not create a release.
- A matching `vVERSION` tag creates a **draft** release on the corresponding forge after all its packaging/consumer jobs and both Linux quality/MSRV jobs succeed. No tag is created by the workflow. The root and FFI package versions must match, and the tag must match that exact version.
- Release write permission is limited to the draft-release job in the canonical repositories (`TeskesLab/libspd` on GitHub and `TeskesLabOrg/libspd` on Gitea). Build jobs use read-only repository access. No release API is invoked from untrusted pull-request builds. The privileged job does not execute downloaded code: it validates the exact expected package names and checksums before attaching an explicit asset list.

### Exact release asset sets

For version `0.0.3`, **both** forges attach these seven package assets:

- `libspd-0.0.3-x86_64-unknown-linux-gnu.zip`
- `libspd-0.0.3-aarch64-unknown-linux-gnu.zip`
- `libspd-0.0.3-x86_64-pc-windows-gnu.zip`
- `libspd1_0.0.3-1_amd64.deb`
- `libspd-dev_0.0.3-1_amd64.deb`
- `libspd1_0.0.3-1_arm64.deb`
- `libspd-dev_0.0.3-1_arm64.deb`

GitHub additionally attaches these three package assets:

- `libspd-0.0.3-x86_64-pc-windows-msvc.zip`
- `libspd-0.0.3-x86_64-apple-darwin.zip`
- `libspd-0.0.3-aarch64-apple-darwin.zip`

Every package asset has its own same-name `.sha256` sidecar. Gitea therefore has **three ZIPs, four Debian packages, and seven checksum files (14 assets)**. GitHub has **six ZIPs, four Debian packages, and ten checksum files (20 assets)**.

Draft creation is a remote publication action; a maintainer must authorize pushing the version tag and triggering release preparation, then inspect assets/notes before separately making the draft public. Existing releases are not silently overwritten; resolve an existing draft deliberately before rerunning a failed publication job.

Gitea 1.27 recognizes the `.github/workflows` layout, so no duplicate `.gitea/workflows` pipeline is needed. GitHub uses the official `actions/upload-artifact@v4` and `actions/download-artifact@v4`; Gitea uses the compatible [`ChristopherHX/gitea-upload-artifact@v4`](https://github.com/ChristopherHX/gitea-upload-artifact) and [`ChristopherHX/gitea-download-artifact@v4`](https://github.com/ChristopherHX/gitea-download-artifact) forks. The Gitea runner must resolve action references through GitHub and use the public external Gitea URL, as the upload fork documents. Its artifact client uses the runner-provided `ACTIONS_RESULTS_URL` for RPC; uploaded/downloaded signed URLs must also be reachable from job containers. Setting only `GITHUB_SERVER_URL` does not fix artifact endpoint reachability.

## Local packaging and consumption

Requires Python 3.11+, Cargo/Rust stable, and native C/C++ development tools for the host platform. Windows also needs an initialized MSVC developer environment. Run from a checkout:

```sh
python scripts/release.py version
python scripts/release.py version --tag v0.0.3
python scripts/release.py package --target x86_64-unknown-linux-gnu
```

Use the matching native target from the table on Windows/macOS. `scripts/release.py` rejects foreign targets rather than cross-building an artifact it cannot execute locally. It uses the repository `target/` directory to avoid disagreement with custom Cargo target paths. Build artifacts and ZIPs stay under `target/`, outside version control.

The native package command builds the FFI crate, collects exact required files, writes the archive/checksum, extracts into a temporary directory, compiles the included C/C++ programs using the extracted header/libraries, and runs them. Consumer failures make the command fail; no hardware test is implied.

### Linux ARM64 cross-packaging and coinstallation

In a Debian 12 amd64 environment, install the foreign runtime/development libraries through Debian multiarch, not by relying only on the cross-compiler's sysroot:

```sh
sudo dpkg --add-architecture arm64
sudo apt-get update
sudo apt-get install --no-install-recommends \
  ca-certificates curl git build-essential python3 dpkg-dev pkg-config binutils xz-utils \
  gcc-aarch64-linux-gnu g++-aarch64-linux-gnu binutils-aarch64-linux-gnu qemu-user \
  libc6-dev:arm64 libgcc-s1:arm64 libstdc++6:arm64
rustup target add aarch64-unknown-linux-gnu
python3 scripts/arm64.py package
python3 scripts/debian.py package --architecture arm64
```

The ZIP command writes `target/dist/libspd-0.0.3-aarch64-unknown-linux-gnu.zip` and its `.sha256` sidecar, using the validated manifest version. It extracts the package, cross-compiles the C static and C++ shared consumers, and runs them with explicit `qemu-aarch64` invocation. Consumer failures fail packaging; a successful cross-build alone is insufficient.

To install and check the arm64 Debian packages, optionally alongside the matching amd64 packages shown above:

```sh
(cd target/dist && sha256sum -c \
  libspd1_0.0.3-1_arm64.deb.sha256 \
  libspd-dev_0.0.3-1_arm64.deb.sha256)
sudo apt-get install --no-install-recommends \
  ./target/dist/libspd1_0.0.3-1_arm64.deb \
  ./target/dist/libspd-dev_0.0.3-1_arm64.deb
python3 scripts/debian.py smoke --architecture arm64
```

Installed ARM64 smoke uses architecture-specific pkg-config metadata and explicit `qemu-aarch64 -L /` with Debian's foreign dynamic loader and libraries. No host binfmt setup is needed.

### Windows GNU cross-packaging

In a Debian 12 amd64 environment, install `ca-certificates curl git build-essential python3 dpkg-dev pkg-config binutils xz-utils gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64 wine wine64` and Rust stable, then run:

```sh
rustup target add x86_64-pc-windows-gnu
python3 scripts/mingw.py package
```

The separate cross-package command writes `target/dist/libspd-0.0.3-x86_64-pc-windows-gnu.zip` and its `.sha256` sidecar (using the validated manifest version). It extracts the archive, compiles Windows C and C++ consumers against the extracted headers and shared/static libraries, and executes those consumers with Wine. A successful Linux cross-build alone is not sufficient: consumer execution failures fail packaging.

For a downloaded package, verify its SHA-256 using your platform's checksum tool, unpack it, add its `include/` to your include paths and its `lib/` to linker paths. See [the FFI guide](ffi.md) for ownership and function contracts, and [build instructions](build.md) for source-checkout examples. Shared-library deployments must make the matching library discoverable to the OS loader. Static linking can require target-specific Rust system libraries; inspect linker diagnostics rather than copying Linux link flags to Windows/macOS.

## Validation boundary

Local execution proves only the package and consumer paths actually exercised. Debian installed-package validation requires Debian 12 with the matching native or multiarch packages; Linux ARM64 checks require GNU AArch64 cross-tools, foreign runtime libraries, and QEMU; Windows GNU smoke checks require MinGW and Wine; native Windows MSVC and macOS paths require their actual native matrix jobs. QEMU/Wine checks do not establish native ARM64/Windows hardware or OS compatibility. Configuration alone is not evidence those jobs have run. Workflows must be pushed to the respective forge and executed before claiming remote release validation. None of the smoke programs establishes DIMM compatibility or permission to program real hardware.

## Platform documentation

- [GitHub workflow permissions](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions): `contents: write` permits release creation; build jobs do not need it.
- [Passing artifacts between jobs](https://docs.github.com/en/actions/tutorials/store-and-share-data): release jobs wait for all ZIP/Debian builds and download their uploaded assets.
- [Gitea workflow permissions](https://docs.gitea.com/1.27/usage/actions/token-permissions/): Gitea 1.27 maps `contents: write` to code and release write access.
