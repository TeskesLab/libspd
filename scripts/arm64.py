#!/usr/bin/env python3
"""Cross-package GNU/Linux ARM64 releases on Linux amd64 and consume them with QEMU."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import tempfile
import zipfile

import release


TARGET = "aarch64-unknown-linux-gnu"
MULTIARCH = "aarch64-linux-gnu"
ARTIFACTS = ("libspd.a", "libspd.so")


def cross_environment(build=None):
    machine = platform.machine().lower()
    machine = {"amd64": "x86_64"}.get(machine, machine)
    if (platform.system(), machine) != ("Linux", "x86_64"):
        raise ValueError("ARM64 cross-packaging requires a Linux amd64 host")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(build or release.ROOT / "target/arm64-build")
    env["CARGO_TERM_COLOR"] = "never"
    env["LC_ALL"] = "C"
    cargo = release.executable("cargo")
    information = release.run(
        [release.executable("rustc"), "-vV"], cwd=release.ROOT, env=env, capture=True,
    )
    hosts = re.findall(r"^host: (\S+)$", information, re.MULTILINE)
    if hosts != ["x86_64-unknown-linux-gnu"]:
        raise ValueError(f"ARM64 cross-packaging requires an amd64 GNU/Linux Rust host, received {hosts!r}")
    compilers = (release.executable("aarch64-linux-gnu-gcc"), release.executable("aarch64-linux-gnu-g++"))
    env["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"] = compilers[0]
    release.executable("readelf")
    # The actual Debian multiarch libraries live under /lib and /usr/lib on
    # this host. Do not use the cross compiler's /usr/aarch64-linux-gnu sysroot
    # or require a privileged binfmt registration on the CI runner.
    runner = (release.executable("qemu-aarch64"), "-L", "/")
    return env, cargo, compilers, runner


def package():
    version = release.release_version()
    env, cargo, compilers, runner = cross_environment()
    release.run(
        [cargo, "build", "--locked", "-p", "libspd-ffi", "--release", "--target", TARGET],
        cwd=release.ROOT, env=env,
    )
    libraries = release.static_libraries(cargo, TARGET, env)
    built = Path(env["CARGO_TARGET_DIR"]) / TARGET / "release"
    release.check_elf(built / "libspd.so", TARGET, env)
    entries = {
        "include/libspd.h": release.ROOT / "include/libspd.h",
        "LICENSE": release.ROOT / "LICENSE",
        "README.md": release.ROOT / "README.md",
        "examples/native.c": release.ROOT / "examples/native.c",
        "examples/native.cpp": release.ROOT / "examples/native.cpp",
    }
    entries.update({f"docs/{name}.md": release.ROOT / f"docs/{name}.md" for name in release.DOCUMENTS})
    entries.update({f"lib/{name}": built / name for name in ARTIFACTS})
    for relative, source in entries.items():
        if not source.is_file() or source.is_symlink():
            raise ValueError(f"missing or non-regular package input for {relative}: {source}")
    dist = release.ROOT / "target/dist"
    dist.mkdir(parents=True, exist_ok=True)
    topdir = f"libspd-{version}-{TARGET}"
    filename = f"{topdir}.zip"
    # Publish neither the archive nor its checksum until consumers built from
    # the extracted payload have actually executed with QEMU.
    with tempfile.TemporaryDirectory(prefix=".libspd-arm64-", dir=dist) as temporary:
        work = Path(temporary)
        staging = work / "staging" / topdir
        for relative, source in entries.items():
            destination = staging / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
        metadata = staging / "release.json"
        metadata.write_text(
            json.dumps({
                "name": "libspd",
                "version": version,
                "target": TARGET,
                "abi_version": 1,
                "rustc_flags_added": [],
                "native_static_libs": libraries,
            }, indent=2, sort_keys=True) + "\n",
            encoding="utf-8", newline="\n",
        )
        entries["release.json"] = metadata
        archive_path = work / filename
        release.write_zip(staging, archive_path, topdir, entries)
        expected = {f"{topdir}/{relative}" for relative in entries}
        with tempfile.TemporaryDirectory(prefix="libspd-arm64-consumer-") as unpacked:
            with zipfile.ZipFile(archive_path) as archive:
                names = archive.namelist()
                if len(names) != len(expected) or set(names) != expected:
                    raise RuntimeError("release ZIP does not contain the exact expected safe file list")
                archive.extractall(unpacked)
            directory = Path(unpacked) / topdir
            release.check_elf(directory / "lib/libspd.so", TARGET, env)
            release.smoke_consumers(directory, TARGET, compilers, libraries, env, runner=runner)
        with archive_path.open("rb") as source:
            checksum = hashlib.file_digest(source, "sha256").hexdigest()
        checksum_path = work / f"{filename}.sha256"
        checksum_path.write_text(f"{checksum}  {filename}\n", encoding="ascii", newline="\n")
        os.replace(archive_path, dist / filename)
        os.replace(checksum_path, dist / f"{filename}.sha256")
    print(dist / filename)
    print(dist / f"{filename}.sha256")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("package", help="cross-build an ARM64 ZIP and run extracted C/C++ consumers with QEMU")
    parser.parse_args()
    try:
        package()
    except (OSError, ValueError, RuntimeError, zipfile.BadZipFile) as error:
        print(f"arm64.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
