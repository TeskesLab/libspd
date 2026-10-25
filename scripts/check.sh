#!/bin/sh
set -eu

# Run from any directory. CARGO_TARGET_DIR is fixed so native artifact paths agree.
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
export CARGO_TARGET_DIR="$root/target"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p libspd --lib
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build -p libspd
rustdoc --edition=2021 --test docs/rust.md \
    -L "dependency=$CARGO_TARGET_DIR/debug/deps" \
    --extern "libspd=$CARGO_TARGET_DIR/debug/liblibspd.rlib"
cargo run --example inspect
cargo run --example legacy
cargo build -p libspd-ffi --release

# This native smoke runner targets Linux. Other targets can build the crates but
# must use their platform's linker conventions (see docs/build.md).
case "$(uname -s)" in
    Linux) ;;
    *) printf '%s\n' 'Native smoke linking currently requires Linux.' >&2; exit 1 ;;
esac

mkdir -p target/native
"${CC:-cc}" -std=c99 -DNDEBUG -Wall -Wextra -Werror -pedantic -Iinclude \
    examples/native.c target/release/libspd.a -ldl -lpthread -lm \
    -o target/native/native-c
"${CXX:-c++}" -std=c++17 -DNDEBUG -Wall -Wextra -Werror -pedantic -Iinclude \
    examples/native.cpp -Ltarget/release -lspd \
    "-Wl,-rpath,\$ORIGIN/../release" -o target/native/native-cpp
target/native/native-c
target/native/native-cpp
printf '%s\n' 'Rust, C static, and C++ shared smoke checks passed.'
