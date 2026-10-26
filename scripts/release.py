#!/usr/bin/env python3
"""Validate release versions and build/consume host-native release ZIPs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import zipfile

if sys.version_info < (3, 11):
    sys.exit("release.py: Python 3.11 or newer is required")

import tomllib


ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64", ("libspd.a", "libspd.so")),
    "x86_64-pc-windows-msvc": ("Windows", "x86_64", ("spd.lib", "spd.dll", "spd.dll.lib")),
    "x86_64-apple-darwin": ("Darwin", "x86_64", ("libspd.a", "libspd.dylib")),
    "aarch64-apple-darwin": ("Darwin", "aarch64", ("libspd.a", "libspd.dylib")),
}
DOCUMENTS = ("ffi", "fields", "legacy", "build", "design", "plan", "rust", "releases")
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def release_version(tag=None):
    versions = []
    for relative in ("Cargo.toml", "ffi/Cargo.toml"):
        with (ROOT / relative).open("rb") as source:
            manifest = tomllib.load(source)
        version = manifest.get("package", {}).get("version")
        if not isinstance(version, str):
            raise ValueError(f"{relative}: package.version must be an explicit string")
        match = SEMVER.fullmatch(version)
        if match is None or (
            match.group(4)
            and any(
                item.isascii() and item.isdigit() and len(item) > 1 and item[0] == "0"
                for item in match.group(4).split(".")
            )
        ):
            raise ValueError(f"{relative}: invalid semantic version {version!r}")
        versions.append(version)
    if versions[0] != versions[1]:
        raise ValueError(
            f"manifest version mismatch: Cargo.toml={versions[0]!r}, "
            f"ffi/Cargo.toml={versions[1]!r}"
        )
    if tag is not None and tag != f"v{versions[0]}":
        raise ValueError(f"release tag {tag!r} must be exactly 'v{versions[0]}'")
    return versions[0]


def run(command, *, cwd, env, capture=False):
    command = [str(argument) for argument in command]
    print("+ " + shlex.join(command), file=sys.stderr, flush=True)
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if capture and result.stdout:
        print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
    if result.returncode != 0:
        raise RuntimeError(f"command exited {result.returncode}: {shlex.join(command)}")
    return result.stdout if capture else None


def executable(name):
    found = shutil.which(name)
    if found is None:
        raise ValueError(f"required executable not found: {name!r}")
    # Keep the invoked basename: cargo/rustc are commonly rustup proxy symlinks.
    return os.path.abspath(found)


def check_host(target, env):
    system, architecture, _ = TARGETS[target]
    machine = platform.machine().lower()
    machine = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    if (platform.system(), machine) != (system, architecture):
        raise ValueError(
            f"target {target!r} requires a native {system}/{architecture} host; "
            f"this Python host is {platform.system()}/{machine}. "
            "Cross-target packaging is unsupported because consumers must execute."
        )
    cargo = executable("cargo")
    rustc = executable("rustc")
    information = run([rustc, "-vV"], cwd=ROOT, env=env, capture=True)
    hosts = re.findall(r"^host: (\S+)$", information, re.MULTILINE)
    if hosts != [target]:
        raise ValueError(f"target {target!r} does not match rustc host: {hosts!r}")
    if system == "Windows":
        compilers = (executable("cl"), executable("cl"))
    else:
        # Overrides are executable paths, not shell commands or flags.
        compilers = (executable(os.environ.get("CC", "cc")), executable(os.environ.get("CXX", "c++")))
    return cargo, compilers


def static_libraries(cargo, target, env):
    # Query the actual crate/toolchain, preserving the order and duplicates rustc
    # requires. In particular, MSVC system libraries are not a guessed list.
    # https://doc.rust-lang.org/rustc/command-line-arguments/print-options.html#native-static-libs
    output = run(
        [cargo, "rustc", "--locked", "-p", "libspd-ffi", "--lib", "--release",
         "--target", target, "--color", "never", "--", "--print", "native-static-libs"],
        cwd=ROOT, env=env, capture=True,
    )
    notes = re.findall(r"^.*?native-static-libs:\s*(.+)$", output, re.MULTILINE)
    if len(notes) != 1:
        raise RuntimeError(
            f"expected exactly one rustc native-static-libs diagnostic for {target}, "
            f"received {len(notes)}; cannot validate the static library"
        )
    libraries = shlex.split(notes[0])
    if not libraries:
        raise RuntimeError(f"rustc reported no native static-library dependencies for {target}")
    return libraries


def smoke_consumers(directory, target, compilers, libraries, env):
    system = TARGETS[target][0]
    include = directory / "include"
    lib = directory / "lib"
    binaries = directory / "bin"
    binaries.mkdir()
    cc, cxx = compilers
    child_env = env.copy()
    if system == "Windows":
        child_env["PATH"] = str(lib) + os.pathsep + child_env.get("PATH", "")
        common = ["/nologo", "/DNDEBUG", "/W3", "/WX", "/MD", f"/I{include}"]
        # Import-library consumers exercise both language ABIs and DLL lookup.
        # A third C consumer proves the packaged static archive is usable too.
        for name, compiler, source, language, library, dependencies in (
            ("native-c-shared", cc, "native.c", ["/TC", "/std:c11"], "spd.dll.lib", []),
            ("native-cpp-shared", cxx, "native.cpp", ["/TP", "/std:c++17", "/EHsc"], "spd.dll.lib", []),
            ("native-c-static", cc, "native.c", ["/TC", "/std:c11"], "spd.lib", libraries),
        ):
            binary = binaries / f"{name}.exe"
            run(
                [compiler, *common, *language, directory / "examples" / source,
                 f"/Fo{binaries / (name + '.obj')}", f"/Fe{binary}", "/link",
                 lib / library, *dependencies],
                cwd=binaries, env=child_env,
            )
            run([binary], cwd=binaries, env=child_env)
    else:
        # Do not let an inherited loader override accidentally satisfy a smoke
        # check with a non-packaged library. The shared consumer uses its rpath.
        for variable in ("LD_LIBRARY_PATH", "LD_PRELOAD", "DYLD_LIBRARY_PATH",
                         "DYLD_FALLBACK_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES"):
            child_env.pop(variable, None)
        common = ["-DNDEBUG", "-Wall", "-Wextra", "-Werror", "-pedantic", f"-I{include}"]
        static_binary = binaries / "native-c-static"
        run(
            [cc, "-std=c99", *common, directory / "examples/native.c", lib / "libspd.a",
             *libraries, "-o", static_binary],
            cwd=binaries, env=child_env,
        )
        run([static_binary], cwd=binaries, env=child_env)
        shared_binary = binaries / "native-cpp-shared"
        loader_path = "$ORIGIN/../lib" if system == "Linux" else "@loader_path/../lib"
        run(
            [cxx, "-std=c++17", *common, directory / "examples/native.cpp", f"-L{lib}",
             "-lspd", f"-Wl,-rpath,{loader_path}", "-o", shared_binary],
            cwd=binaries, env=child_env,
        )
        run([shared_binary], cwd=binaries, env=child_env)
    print(f"Extracted-archive C/C++ static/shared consumers passed for {target}.", file=sys.stderr)


def write_zip(source, destination, topdir, entries):
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative in sorted(entries):
            info = zipfile.ZipInfo(f"{topdir}/{relative}", date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | 0o644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            with (source / relative).open("rb") as input_file, archive.open(info, "w") as output_file:
                shutil.copyfileobj(input_file, output_file)


def package(target):
    version = release_version()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    env["CARGO_TERM_COLOR"] = "never"
    added_flags = []
    if TARGETS[target][0] == "Darwin":
        # Ask rustc to emit @rpath/libspd.dylib at link time. Post-link install
        # name edits would invalidate the ARM64 dylib's ad-hoc code signature.
        added_flags = ["-C", "rpath=yes"]
        if "CARGO_ENCODED_RUSTFLAGS" in env:
            previous = env["CARGO_ENCODED_RUSTFLAGS"]
            env["CARGO_ENCODED_RUSTFLAGS"] = (
                previous + ("\x1f" if previous else "") + "\x1f".join(added_flags)
            )
        elif "RUSTFLAGS" in env:
            env["RUSTFLAGS"] += " -C rpath=yes"
        else:
            variable = "CARGO_TARGET_" + target.upper().replace("-", "_") + "_RUSTFLAGS"
            env[variable] = (env.get(variable, "") + " -C rpath=yes").strip()
    cargo, compilers = check_host(target, env)
    run(
        [cargo, "build", "--locked", "-p", "libspd-ffi", "--release", "--target", target],
        cwd=ROOT, env=env,
    )
    libraries = static_libraries(cargo, target, env)
    entries = {
        "include/libspd.h": ROOT / "include/libspd.h",
        "LICENSE": ROOT / "LICENSE",
        "README.md": ROOT / "README.md",
        "examples/native.c": ROOT / "examples/native.c",
        "examples/native.cpp": ROOT / "examples/native.cpp",
    }
    entries.update({f"docs/{name}.md": ROOT / f"docs/{name}.md" for name in DOCUMENTS})
    entries.update({
        f"lib/{name}": ROOT / "target" / target / "release" / name
        for name in TARGETS[target][2]
    })
    for relative, source in entries.items():
        if not source.is_file() or source.is_symlink():
            raise ValueError(f"missing or non-regular package input for {relative}: {source}")
    dist = ROOT / "target/dist"
    dist.mkdir(parents=True, exist_ok=True)
    topdir = f"libspd-{version}-{target}"
    filename = f"{topdir}.zip"
    # Nothing reaches the published artifact names until the unpacked ZIP has
    # compiled and executed successfully, including all required native paths.
    with tempfile.TemporaryDirectory(prefix=".libspd-release-", dir=dist) as temporary:
        work = Path(temporary)
        staging = work / "staging" / topdir
        for relative, source in entries.items():
            destination = staging / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
        metadata = staging / "release.json"
        metadata.write_text(
            json.dumps(
                {
                    "name": "libspd",
                    "version": version,
                    "target": target,
                    "abi_version": 1,
                    "rustc_flags_added": added_flags,
                    "native_static_libs": libraries,
                },
                indent=2,
                sort_keys=True,
            ) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        entries["release.json"] = metadata
        archive_path = work / filename
        write_zip(staging, archive_path, topdir, entries)
        expected = {f"{topdir}/{relative}" for relative in entries}
        with tempfile.TemporaryDirectory(prefix="libspd-consumer-") as unpacked:
            with zipfile.ZipFile(archive_path) as archive:
                names = archive.namelist()
                if len(names) != len(expected) or set(names) != expected:
                    raise RuntimeError("release ZIP does not contain the exact expected safe file list")
                archive.extractall(unpacked)
            smoke_consumers(Path(unpacked) / topdir, target, compilers, libraries, env)
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
    version_parser = commands.add_parser("version", help="validate both manifest versions and an optional tag")
    version_parser.add_argument("--tag", help="exact release tag, including the v prefix")
    package_parser = commands.add_parser("package", help="build a native ZIP and run unpacked C/C++ consumers")
    package_parser.add_argument("--target", required=True, choices=TARGETS)
    args = parser.parse_args()
    try:
        if args.command == "version":
            print(release_version(args.tag))
        else:
            package(args.target)
    except (OSError, ValueError, RuntimeError, zipfile.BadZipFile) as error:
        print(f"release.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
