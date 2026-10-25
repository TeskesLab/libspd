# Rust API guide

Use the safe `libspd` crate directly from Rust. It is `no_std`, allocation-free,
and forbids unsafe code. It borrows your bytes rather than owning a device,
allocating an image, or opening a file. The separate `libspd-ffi` crate is the
unsafe native boundary for C/foreign-language consumers, not a required Rust
wrapper. See [the FFI guide](ffi.md) only when crossing that boundary.

## Get the API reference

From this checkout:

```sh
cargo doc -p libspd --no-deps --open
cargo test -p libspd --doc
```

The crate-level rustdoc includes a runnable quick start. Every public type,
variant, field, associated type, and method has a documentation contract;
`missing_docs` and broken intra-doc links are denied. Public docs include errors,
ownership, units, supported interpretation, and mutation behavior, not just names.

For a sibling checkout dependency use `libspd = { path = "../libspd" }`.
The declared MSRV is Rust 1.81. Registry publication is intended, not established
by a manifest version or docs.rs URL; see [publication notes](build.md#publication-metadata).

## Choose an operation

| Need | Rust API |
|---|---|
| Classify three available bytes | `Image::memory_type()` |
| Read generation-aware raw identity | `Image::identity()` |
| Inspect the literal first four bytes | `Image::header()` |
| Read/edit selected timings | `Image::timing()`, `ImageMut::set_timing()` |
| Borrow raw manufacturing fields | `manufacturing()` for DDR4/DDR5; `legacy_manufacturing()` earlier |
| Edit raw serial or printable part number | `ImageMut::set_serial_number()`, `set_part_number()` |
| Inspect/explicitly repair integrity | `checksum()`, `base_crc()`, `module_crc()` and corresponding repair methods |
| Move uninterpreted bytes | `Image::read()`, `ImageMut::write()`, `Image::as_bytes()` |
| Supply external hardware/storage | Application implementation of `Read` and optionally `Write` |

Constructing an `Image` does not validate the image. A known memory type does not
imply support for every revision/module layout. Use [the modern matrix](fields.md)
and [legacy matrix](legacy.md) for exact supported fields and revisions.

**Legacy trap:** `Header::revision` is raw byte 1 and `module_type_raw` is raw
byte 3. Before DDR3 these are not semantic revision/module fields. Use `Identity`
for generation-aware positions and its `Option<u8>` for absent module types.

## Decode, edit, and explicitly repair

This example creates only a few meaningful synthetic DDR5 fields. It is not a
vendor dump, valid complete module description, or hardware-programming policy.
The metadata part number is borrowed and fixed-width, not a Rust string.

```rust
use libspd::{Error, Image, ImageMut, MemoryType, TimingField};

fn main() -> Result<(), Error> {
    let mut bytes = [0u8; 555];
    bytes[..4].copy_from_slice(&[0x30, 0x10, 0x12, 0x02]);
    bytes[70..73].copy_from_slice(&[0x88, 0x13, 8]); // 5000 ps and 8 nCK.

    let image = Image::new(&bytes);
    assert_eq!(image.identity()?.memory_type, MemoryType::Ddr5);
    let mut delay = image.timing(TimingField::TrrdL)?;
    assert_eq!(delay.picoseconds, 5000);
    assert_eq!(delay.minimum_cycles, Some(8));

    // The immutable borrow is no longer used before the mutable borrow starts.
    let mut edited = ImageMut::new(&mut bytes);
    delay.picoseconds = 6000; // Keep the independent nCK constraint.
    edited.set_timing(TimingField::TrrdL, delay)?;
    edited.set_part_number(b"SYNTHETIC-RUST")?;
    edited.set_serial_number([1, 2, 3, 4])?;
    let view = edited.as_image();
    let metadata = view.manufacturing()?;
    assert_eq!(metadata.part_number.len(), 30);
    assert_eq!(&metadata.part_number[..14], b"SYNTHETIC-RUST");
    assert_eq!(metadata.serial, [1, 2, 3, 4]);
    assert_eq!(view.timing(TimingField::TrrdL)?, delay);
    assert_eq!(view.base_crc()?.stored, 0); // No automatic repair.

    // Explicit policy for this synthetic example; validation is a separate job.
    edited.repair_base_crc()?;
    assert!(edited.as_image().base_crc()?.is_valid());
    assert_eq!(&edited.as_image().as_bytes()[70..73], &[0x70, 0x17, 8]);
    Ok(())
}
```

Timings use exact integer picoseconds; DDR5 nanosecond fields convert explicitly.
`minimum_cycles` is a separate constraint, not a derived value. `Some(0)` differs
from `None`. Preserve it when editing only duration. Setters reject missing or
extraneous cycle constraints and values not representable without rounding.

`Image<'a>` and manufacturing slices borrow the original input. `ImageMut`
exclusively borrows mutable storage; `as_image()` borrows that mutable view only
for its own borrow duration. Use the exact bytes to serialize/save an edited
snapshot. No library object owns or frees your buffer, and parsing never invokes
an adapter. Clone into application-owned working storage if you need an untouched
original for diagnostics or comparison.

## External adapters are application code

Both traits are synchronous whole-buffer operations at flat image offsets. An
associated error type preserves application-specific diagnostics. Implementing
`Write` also requires `Read`, sharing the associated error type. This complete
memory-backed example exercises the traits without claiming hardware support:

```rust
use libspd::{Error, Image, ImageMut, Read, Write};

struct Memory([u8; 8]);
impl Read for Memory {
    type Error = Error;
    fn read(&mut self, offset: usize, output: &mut [u8]) -> Result<(), Error> {
        Image::new(&self.0).read(offset, output)
    }
}
impl Write for Memory {
    fn write(&mut self, offset: usize, input: &[u8]) -> Result<(), Error> {
        ImageMut::new(&mut self.0).write(offset, input)
    }
}
fn main() -> Result<(), Error> {
    let mut adapter = Memory([0; 8]);
    adapter.write(2, &[0xa5, 0x5a])?;
    let mut snapshot = [0; 2];
    adapter.read(2, &mut snapshot)?;
    assert_eq!(snapshot, [0xa5, 0x5a]);
    Ok(())
}
```

A physical backend owns addresses, paging, transfer limits, locking, retries,
write-cycle timing, protection, backup, and read-back verification. Success means
the whole buffer transferred; an error may follow partial output or physical
writes. The library does not promise adapter rollback. The stronger atomicity of
`ImageMut` buffer edits must not be assumed for arbitrary `Write` implementations.
See `examples/inspect.rs` and `examples/legacy.rs` for executable integration paths.

## Errors, preservation, and integrity policy

- `Truncated` means the requested operation lacks required bytes; independent
  sections can still be readable. `OutOfBounds` includes raw-range overflow.
- Unsupported memory/revision/module/field/timebase errors mean the selected
  interpretation is unavailable. Keep raw data; do not force an assumed layout.
- `InvalidEncoding { offset }` reports a byte in the image/proposed encoding.
  `ValueOutOfRange` means an edit cannot be represented exactly. Failed buffer
  edits preserve every byte. No-op timing setters retain alternative encodings.
- Date, manufacturer, serial, and revision data remain raw. Part getters preserve
  unknown/non-UTF-8 bytes. Converting to text or decoding JEP106/BCD is application
  policy; part setters explicitly accept bounded printable ASCII and space-pad.
- A successful integrity check can return `is_valid() == false`; that mismatch
  is not a decoding error. Parsing does not require integrity validity.
- Pre-DDR3 uses an additive checksum; DDR3 has bit-selected base CRC coverage;
  DDR4 has independent base/module CRCs; DDR5 currently exposes only base CRC.
  Repair the applicable domain explicitly. Raw writes can deliberately change
  any requested bytes, including integrity storage; they do not recompute it.

A representable image with correct checksums can still describe impossible memory
settings. No semantic lint, full-generation conformance, or safe-flashing claim
is implied. See [architecture and integration](design.md) for product boundaries.
