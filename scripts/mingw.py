#!/usr/bin/env python3
"""Cross-package Windows GNU releases on Linux x86_64 and consume them with Wine."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import zipfile

from release import DOCUMENTS, ROOT, executable, release_version, run, static_libraries, write_zip


TARGET = "x86_64-pc-windows-gnu"
ARTIFACTS = ("spd.dll", "libspd.dll.a", "libspd.a")
# Keep compiler runtimes in the binaries, rather than requiring a particular
# MinGW installation on a consumer's Windows machine. Windows system DLLs
# (including the Windows CRT) remain normal operating-system dependencies.
RUST_FLAGS = ("-C", "link-arg=-static", "-C", "link-arg=-static-libgcc")
COMPILER_FLAGS = ("-static", "-static-libgcc", "-static-libstdc++")
COMPILER_RUNTIME = re.compile(
    r"^(?:lib(?:gcc_s_|stdc\+\+|winpthread|ssp|gomp|quadmath|atomic)|"
    r"(?:lib)?(?:unwind|c\+\+)(?:[-.]|$))", re.IGNORECASE,
)


def stdout(command, *, cwd, env):
    # Tool diagnostics stay on stderr; they are not part of a path/import table.
    command = [str(argument) for argument in command]
    print("+ " + shlex.join(command), file=sys.stderr, flush=True)
    return subprocess.run(
        command, cwd=cwd, env=env, stdout=subprocess.PIPE,
        text=True, encoding="utf-8", check=True,
    ).stdout


def imports(binary, objdump, env):
    output = stdout([objdump, "-p", binary], cwd=binary.parent, env=env)
    if not re.search(r"file format pei-x86-64\b", output):
        raise RuntimeError(f"not an x86_64 Windows PE binary: {binary}")
    dependencies = re.findall(r"^\s*DLL Name:\s*(\S+)\s*$", output, re.MULTILINE)
    if not dependencies:
        raise RuntimeError(f"no PE imports found in {binary}")
    runtimes = [name for name in dependencies if COMPILER_RUNTIME.match(name)]
    if runtimes:
        raise RuntimeError(
            f"compiler runtimes were not linked statically into {binary.name}: {runtimes}"
        )
    return {name.lower() for name in dependencies}


def smoke_consumers(directory, cc, cxx, objdump, wine, winepath, libraries, env):
    binaries = directory / "bin"
    binaries.mkdir()
    include = directory / "include"
    lib = directory / "lib"
    common = ["-DNDEBUG", "-Wall", "-Wextra", "-Werror", "-pedantic", f"-I{include}"]
    static_binary = binaries / "native-c-static.exe"
    run(
        [cc, "-std=c99", *common, directory / "examples/native.c", lib / "libspd.a",
         *libraries, *COMPILER_FLAGS, "-o", static_binary],
        cwd=binaries, env=env,
    )
    if "spd.dll" in imports(static_binary, objdump, env):
        raise RuntimeError("the static consumer unexpectedly imports spd.dll")
    shared_binary = binaries / "native-cpp-shared.exe"
    run(
        [cxx, "-std=c++17", *common, directory / "examples/native.cpp",
         lib / "libspd.dll.a", *COMPILER_FLAGS, "-o", shared_binary],
        cwd=binaries, env=env,
    )
    if "spd.dll" not in imports(shared_binary, objdump, env):
        raise RuntimeError("the DLL consumer does not import the packaged spd.dll")
    # Never let an existing Wine prefix or an inherited Windows search path
    # satisfy the check with an unrelated DLL. A fresh prefix has only Wine's
    # built-in Windows libraries; spd.dll must come from this extracted ZIP.
    with tempfile.TemporaryDirectory(prefix="libspd-wine-") as temporary:
        child_env = env.copy()
        for variable in ("WINEPREFIX", "WINEARCH", "WINEPATH", "WINEDLLOVERRIDES",
                         "WINEDLLPATH", "WINELOADER", "WINESERVER"):
            child_env.pop(variable, None)
        home = Path(temporary) / "home"
        home.mkdir()
        child_env.update({
            "WINEPREFIX": str(Path(temporary) / "prefix"),
            "HOME": str(home),
            "XDG_CACHE_HOME": str(home / ".cache"),
            "WINEARCH": "win64",
            "WINEDEBUG": "-all",
            "WINEDLLOVERRIDES": "winemenubuilder.exe=d;spd=n",
        })
        try:
            # Initialize before capturing winepath: a new prefix prints setup
            # diagnostics that must not be mistaken for its stdout path.
            run([wine, "wineboot.exe", "--init"], cwd=directory, env=child_env)
            windows_lib = stdout([winepath, "-w", lib], cwd=directory, env=child_env)
            windows_lib = windows_lib.strip()
            if not re.fullmatch(r"[A-Za-z]:\\[^\r\n]*", windows_lib):
                raise RuntimeError(f"winepath did not return one absolute Windows path: {windows_lib!r}")
            child_env["WINEPATH"] = windows_lib
            run([wine, static_binary], cwd=binaries, env=child_env)
            run([wine, shared_binary], cwd=binaries, env=child_env)
        finally:
            # Wine keeps background services alive after an executable exits.
            # Stop only this isolated prefix before its directory is removed.
            wineserver = executable("wineserver")
            run([wineserver, "-k"], cwd=directory, env=child_env)
            run([wineserver, "-w"], cwd=directory, env=child_env)
    print(f"Extracted-archive C static/C++ DLL consumers passed for {TARGET}.", file=sys.stderr)


def package():
    machine = {"amd64": "x86_64"}.get(platform.machine().lower(), platform.machine().lower())
    if (platform.system(), machine) != ("Linux", "x86_64"):
        raise ValueError("MinGW packaging requires a Linux x86_64 host")
    version = release_version()
    cargo = executable("cargo")
    cc = executable("x86_64-w64-mingw32-gcc")
    cxx = executable("x86_64-w64-mingw32-g++")
    objdump = executable("x86_64-w64-mingw32-objdump")
    wine = executable("wine")
    winepath = executable("winepath")
    executable("wineserver")
    env = os.environ.copy()
    build = ROOT / "target/mingw-build"
    env["CARGO_TARGET_DIR"] = str(build)
    env["CARGO_TERM_COLOR"] = "never"
    env["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER"] = cc
    if "CARGO_ENCODED_RUSTFLAGS" in env:
        previous = env["CARGO_ENCODED_RUSTFLAGS"]
        env["CARGO_ENCODED_RUSTFLAGS"] = (
            previous + ("\x1f" if previous else "") + "\x1f".join(RUST_FLAGS)
        )
    elif "RUSTFLAGS" in env:
        env["RUSTFLAGS"] += " " + shlex.join(RUST_FLAGS)
    else:
        variable = "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS"
        env[variable] = (env.get(variable, "") + " " + shlex.join(RUST_FLAGS)).strip()
    run(
        [cargo, "build", "--locked", "-p", "libspd-ffi", "--release", "--target", TARGET],
        cwd=ROOT, env=env,
    )
    libraries = static_libraries(cargo, TARGET, env)
    entries = {
        "include/libspd.h": ROOT / "include/libspd.h",
        "LICENSE": ROOT / "LICENSE",
        "README.md": ROOT / "README.md",
        "examples/native.c": ROOT / "examples/native.c",
        "examples/native.cpp": ROOT / "examples/native.cpp",
    }
    entries.update({f"docs/{name}.md": ROOT / f"docs/{name}.md" for name in DOCUMENTS})
    entries.update({f"lib/{name}": build / TARGET / "release" / name for name in ARTIFACTS})
    for relative, source in entries.items():
        if not source.is_file() or source.is_symlink():
            raise ValueError(f"missing or non-regular package input for {relative}: {source}")
    imports(entries["lib/spd.dll"], objdump, env)
    dist = ROOT / "target/dist"
    dist.mkdir(parents=True, exist_ok=True)
    topdir = f"libspd-{version}-{TARGET}"
    filename = f"{topdir}.zip"
    # Keep both final artifact names untouched until the extracted archive has
    # passed real cross-compiled consumers, including actual DLL loading.
    with tempfile.TemporaryDirectory(prefix=".libspd-mingw-", dir=dist) as temporary:
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
                "rustc_flags_added": list(RUST_FLAGS),
                "native_static_libs": libraries,
                "consumer_link_flags": list(COMPILER_FLAGS),
                "compiler_runtimes": "statically linked",
            }, indent=2, sort_keys=True) + "\n",
            encoding="utf-8", newline="\n",
        )
        entries["release.json"] = metadata
        archive_path = work / filename
        write_zip(staging, archive_path, topdir, entries)
        expected = {f"{topdir}/{relative}" for relative in entries}
        with tempfile.TemporaryDirectory(prefix="libspd-mingw-consumer-") as unpacked:
            with zipfile.ZipFile(archive_path) as archive:
                names = archive.namelist()
                if len(names) != len(expected) or set(names) != expected:
                    raise RuntimeError("release ZIP does not contain the exact expected safe file list")
                archive.extractall(unpacked)
            smoke_consumers(
                Path(unpacked) / topdir, cc, cxx, objdump, wine, winepath, libraries, env,
            )
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
    commands.add_parser("package", help="cross-build a ZIP and run its extracted consumers with Wine")
    parser.parse_args()
    try:
        package()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"mingw.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
