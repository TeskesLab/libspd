#!/usr/bin/env python3
"""Build native Debian amd64 packages or exercise their installed C/C++ interface."""

import argparse
from email.utils import formatdate
import gzip
import hashlib
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import sys
import tempfile
import time

import release


TARGET = "x86_64-unknown-linux-gnu"
ARCHITECTURE = "amd64"
MULTIARCH = "x86_64-linux-gnu"
SONAME = "libspd.so.1"
MAINTAINER = "Lucas Teske <lucas@teske.com.br>"


def debian_version(version):
    # Debian sorts ~ before the final release; retain SemVer build metadata.
    upstream, separator, metadata = version.partition("+")
    return upstream.replace("-", "~", 1) + separator + metadata + "-1"


def native_environment():
    if platform.system() != "Linux" or platform.freedesktop_os_release().get("ID") != "debian":
        raise ValueError("Debian packages require a native Debian host; CI uses Debian 12/bookworm")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(release.ROOT / "target/debian-build")
    env["CARGO_TERM_COLOR"] = "never"
    env["LC_ALL"] = "C"
    cargo, compilers = release.check_host(TARGET, env)
    for field, expected in (("DEB_HOST_ARCH", ARCHITECTURE), ("DEB_HOST_MULTIARCH", MULTIARCH)):
        actual = release.run(
            [release.executable("dpkg-architecture"), f"-q{field}"],
            cwd=release.ROOT, env=env, capture=True,
        ).strip()
        if actual != expected:
            raise ValueError(f"{field} must be {expected!r}, received {actual!r}; cross-building is unsupported")
    return env, cargo, compilers


def write_text(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def check_soname(library, env):
    dynamic = release.run(
        [release.executable("readelf"), "-d", library],
        cwd=release.ROOT, env=env, capture=True,
    )
    if re.findall(r"\(SONAME\).*?\[(.*?)\]", dynamic) != [SONAME]:
        raise RuntimeError(f"Debian shared library must have SONAME {SONAME}")


def finish_tree(tree, name, version, dependencies, section):
    control = tree / "DEBIAN"
    control.mkdir(exist_ok=True)
    size = sum(path.lstat().st_size for path in (tree / "usr").rglob("*") if not path.is_dir())
    write_text(control / "control", f"""Package: {name}
Source: libspd
Version: {version}
Architecture: {ARCHITECTURE}
Maintainer: {MAINTAINER}
Section: {section}
Priority: optional
Multi-Arch: same
Depends: {dependencies}
Installed-Size: {(size + 1023) // 1024}
Homepage: https://github.com/TeskesLab/libspd
Description: hardware-independent SPD parsing and manipulation ({'runtime' if name == 'libspd1' else 'development files'})
 Parses and edits in-memory SPD images through a C ABI usable from C and C++.
 Applications provide hardware access; this package contains no hardware driver.
""")
    digests = []
    for path in sorted((tree / "usr").rglob("*")):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as source:
                digest = hashlib.file_digest(source, "md5").hexdigest()
            digests.append(f"{digest}  {path.relative_to(tree).as_posix()}\n")
    write_text(control / "md5sums", "".join(digests))
    if name == "libspd1":
        write_text(control / "shlibs", f"libspd 1 libspd1 (>= {version})\n")
        write_text(control / "triggers", "activate-noawait ldconfig\n")
    # Package permissions must not depend on the builder's umask. Symlinks are
    # deliberately relative, including the dev link resolved by libspd1.
    tree.chmod(0o755)
    for path in tree.rglob("*"):
        if not path.is_symlink():
            path.chmod(0o755 if path.is_dir() else 0o644)


def package():
    upstream = release.release_version()
    version = debian_version(upstream)
    env, cargo, compilers = native_environment()
    for tool in ("dpkg-deb", "dpkg-shlibdeps", "readelf"):
        release.executable(tool)
    # Separate output and flags from ZIP builds: those keep their original
    # unversioned library layout, while Debian requires an ABI SONAME.
    previous = env.get("CARGO_ENCODED_RUSTFLAGS")
    if previous is None:
        variable = "CARGO_TARGET_" + TARGET.upper().replace("-", "_") + "_RUSTFLAGS"
        previous = "\x1f".join(shlex.split(env.get("RUSTFLAGS", env.get(variable, ""))))
    flags = ["-C", f"link-arg=-Wl,-soname,{SONAME}"]
    env["CARGO_ENCODED_RUSTFLAGS"] = previous + ("\x1f" if previous else "") + "\x1f".join(flags)
    release.run(
        [cargo, "build", "--locked", "-p", "libspd-ffi", "--release", "--target", TARGET],
        cwd=release.ROOT, env=env,
    )
    libraries = release.static_libraries(cargo, TARGET, env)
    built = Path(env["CARGO_TARGET_DIR"]) / TARGET / "release"
    check_soname(built / "libspd.so", env)
    dist = release.ROOT / "target/dist"
    dist.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".libspd-debian-", dir=dist) as temporary:
        work = Path(temporary)
        runtime = work / "debian/libspd1"
        development = work / "debian/libspd-dev"
        runtime_lib = runtime / "usr/lib" / MULTIARCH
        dev_lib = development / "usr/lib" / MULTIARCH
        runtime_lib.mkdir(parents=True)
        dev_lib.mkdir(parents=True)
        shutil.copyfile(built / "libspd.so", runtime_lib / SONAME)
        shutil.copyfile(built / "libspd.a", dev_lib / "libspd.a")
        (dev_lib / "libspd.so").symlink_to(SONAME)
        header = development / "usr/include/libspd.h"
        header.parent.mkdir(parents=True)
        shutil.copyfile(release.ROOT / "include/libspd.h", header)
        epoch = int(env["SOURCE_DATE_EPOCH"]) if "SOURCE_DATE_EPOCH" in env else time.time()
        changelog = (
            f"libspd ({version}) bookworm; urgency=medium\n\n"
            "  * Build the libspd runtime and C/C++ development packages.\n\n"
            f" -- {MAINTAINER}  {formatdate(epoch, localtime=False)}\n"
        )
        for name, tree in (("libspd1", runtime), ("libspd-dev", development)):
            doc = tree / "usr/share/doc" / name
            doc.mkdir(parents=True)
            shutil.copyfile(release.ROOT / "LICENSE", doc / "copyright")
            (doc / "changelog.Debian.gz").write_bytes(gzip.compress(changelog.encode("utf-8"), mtime=0))
        doc = development / "usr/share/doc/libspd-dev"
        shutil.copyfile(release.ROOT / "README.md", doc / "README.md")
        (doc / "docs").mkdir()
        for name in release.DOCUMENTS:
            shutil.copyfile(release.ROOT / f"docs/{name}.md", doc / f"docs/{name}.md")
        (doc / "examples").mkdir()
        for name in ("native.c", "native.cpp"):
            shutil.copyfile(release.ROOT / "examples" / name, doc / "examples" / name)
        write_text(dev_lib / "pkgconfig/libspd.pc", f"""prefix=/usr
libdir=${{prefix}}/lib/{MULTIARCH}
includedir=${{prefix}}/include

Name: libspd
Description: Hardware-independent SPD parsing and manipulation
Version: {upstream}
URL: https://github.com/TeskesLab/libspd
Libs: -L${{libdir}} -lspd
Libs.private: {shlex.join(libraries)}
Cflags: -I${{includedir}}
""")
        # dpkg-shlibdeps reads the Debian host's installed library metadata. Do
        # not guess glibc/GCC minimums or generate Ubuntu dependency versions.
        write_text(work / "debian/control", f"""Source: libspd
Section: libs
Priority: optional
Maintainer: {MAINTAINER}

Package: libspd1
Architecture: any
Description: hardware-independent SPD library
""")
        # dpkg-shlibdeps identifies the package root by this directory.
        (runtime / "DEBIAN").mkdir()
        output = release.run(
            [release.executable("dpkg-shlibdeps"), "-O", f"-e{(runtime_lib / SONAME).relative_to(work)}"],
            cwd=work, env=env, capture=True,
        )
        dependencies = re.findall(r"^shlibs:Depends=(.+)$", output, re.MULTILINE)
        if len(dependencies) != 1:
            raise RuntimeError("dpkg-shlibdeps did not report exactly one runtime dependency list")
        finish_tree(runtime, "libspd1", version, dependencies[0], "libs")
        finish_tree(development, "libspd-dev", version, f"libspd1 (= {version})", "libdevel")
        archives = []
        extracted = work / "extracted"
        extracted.mkdir()
        for name, tree in (("libspd1", runtime), ("libspd-dev", development)):
            archive = work / f"{name}_{version}_{ARCHITECTURE}.deb"
            release.run(
                [release.executable("dpkg-deb"), "--build", "--root-owner-group", "-Zxz", tree, archive],
                cwd=work, env=env,
            )
            release.run([release.executable("dpkg-deb"), "--extract", archive, extracted], cwd=work, env=env)
            archives.append(archive)
        # Reuse the native consumer programs on the payload actually extracted
        # from both .deb files, not on the original Cargo build outputs.
        (extracted / "lib").symlink_to(Path("usr/lib") / MULTIARCH)
        (extracted / "include").symlink_to("usr/include")
        (extracted / "examples").symlink_to("usr/share/doc/libspd-dev/examples")
        release.smoke_consumers(extracted, TARGET, compilers, libraries, env)
        for archive in archives:
            with archive.open("rb") as source:
                digest = hashlib.file_digest(source, "sha256").hexdigest()
            sidecar = archive.with_name(archive.name + ".sha256")
            write_text(sidecar, f"{digest}  {archive.name}\n")
        for archive in archives:
            sidecar = archive.with_name(archive.name + ".sha256")
            os.replace(archive, dist / archive.name)
            os.replace(sidecar, dist / sidecar.name)
            print(dist / archive.name)
            print(dist / sidecar.name)


def smoke_installed():
    upstream = release.release_version()
    version = debian_version(upstream)
    env, _, compilers = native_environment()
    for variable in ("PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR", "PKG_CONFIG_SYSROOT_DIR", "LD_LIBRARY_PATH", "LD_PRELOAD"):
        env.pop(variable, None)
    pkg_config = release.executable("pkg-config")
    for name in ("libspd1", "libspd-dev"):
        installed = release.run(
            [release.executable("dpkg-query"), "-W", "-f=${Version}", f"{name}:{ARCHITECTURE}"],
            cwd=release.ROOT, env=env, capture=True,
        ).strip()
        if installed != version:
            raise RuntimeError(f"installed {name} version {installed!r} does not match {version!r}")
    found = release.run([pkg_config, "--modversion", "libspd"], cwd=release.ROOT, env=env, capture=True).strip()
    if found != upstream:
        raise RuntimeError(f"installed pkg-config version {found!r} does not match {upstream!r}")
    libdir = Path(release.run([pkg_config, "--variable=libdir", "libspd"], cwd=release.ROOT, env=env, capture=True).strip())
    if libdir != Path("/usr/lib") / MULTIARCH:
        raise RuntimeError(f"unexpected installed library directory: {libdir}")
    check_soname(libdir / SONAME, env)
    cflags = shlex.split(release.run([pkg_config, "--cflags", "libspd"], cwd=release.ROOT, env=env, capture=True))
    # Minimal images can exclude /usr/share/doc during installation. Fixtures
    # stay in the checkout; headers and libraries must come from the packages.
    examples = release.ROOT / "examples"
    with tempfile.TemporaryDirectory(prefix="libspd-debian-installed-") as temporary:
        work = Path(temporary)
        for language, compiler, source, standard, static in (
            ("c-static", compilers[0], "native.c", "-std=c99", True),
            ("cpp-shared", compilers[1], "native.cpp", "-std=c++17", False),
        ):
            options = ["--libs", "libspd"]
            if static:
                options.insert(0, "--static")
            libraries = shlex.split(release.run([pkg_config, *options], cwd=work, env=env, capture=True))
            if "-lspd" not in libraries:
                raise RuntimeError("installed pkg-config metadata does not link libspd")
            if static:
                libraries = [str(libdir / "libspd.a") if flag == "-lspd" else flag for flag in libraries]
            binary = work / language
            release.run(
                [compiler, standard, "-DNDEBUG", "-Wall", "-Wextra", "-Werror", "-pedantic",
                 *cflags, examples / source, *libraries, "-o", binary],
                cwd=work, env=env,
            )
            dynamic = release.run([release.executable("readelf"), "-d", binary], cwd=work, env=env, capture=True)
            needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic)
            if (SONAME in needed) == static:
                raise RuntimeError(f"{language}: incorrect static/shared linkage: {needed}")
            # No rpath or LD_LIBRARY_PATH: this proves package installation and
            # the ldconfig trigger make the ABI library discoverable normally.
            release.run([binary], cwd=work, env=env)
    print("Installed Debian pkg-config, C static, and C++ shared consumers passed.", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("version", "package", "smoke"))
    args = parser.parse_args()
    try:
        if args.command == "version":
            print(debian_version(release.release_version()))
        elif args.command == "package":
            package()
        else:
            smoke_installed()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"debian.py: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
