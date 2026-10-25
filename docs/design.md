# Architecture and integration

## Packages

The root `libspd` package is a `no_std`, allocation-free Rust library. It forbids unsafe code and works entirely on borrowed slices. The `libspd-ffi` package builds the native `spd` library (`libspd.a` and `libspd.so` on Linux), reusing the root crate. Only this boundary crate uses raw pointers and the standard library. There are no external dependencies.

C++ applications include the C header directly. The native API does not allocate memory, open devices, or retain application pointers.

Rust consumers can start with the [Rust API guide](rust.md) and executable crate-level rustdoc examples. All public Rust items are covered by missing-documentation checks.

## Image operations

`Image::new` accepts an arbitrary byte slice, including a partial or empty image. It does not assert that the bytes describe a valid module. `memory_type()` needs three bytes. `header()` preserves the raw first-four-byte view; its byte-1 and byte-3 slots must not be interpreted as revision/module type for pre-DDR3 formats. Use `identity()` for generation-aware raw revision and optional module type. Recognizing a generation does not imply complete layout support.

`read()` and `ImageMut::write()` check the entire requested range before copying. Zero-length operations are valid only at offsets within or exactly at the end of the image. Arithmetic overflow and out-of-range accesses return errors; a failed raw edit changes nothing. Unknown and reserved bytes outside the edit are preserved exactly.

`base_crc()` supports these domains, independent of the input's total length:

| Memory type | Covered bytes (inclusive) | Stored little-endian CRC |
|---|---|---|
| DDR3, byte 0 bit 7 set | 0–116 | 126–127 |
| DDR3, byte 0 bit 7 clear | 0–125 | 126–127 |
| DDR4 | 0–125 | 126–127 |
| DDR5 | 0–509 | 510–511 |

The algorithm is CRC-16/XMODEM: polynomial `0x1021`, initial value zero, no reflection, no final XOR. The independent check vector `123456789` produces `0x31c3`. A truncated domain returns an error; an unknown memory type is unsupported for base CRC but can still have a raw header. No full-image length, SPD-size encoding, revision validity, or semantic check is implied.

`repair_base_crc()` changes only the stored base CRC bytes, after validating that the supported domain is present. Edits do not invoke it. Parsing does not reject a mismatched CRC. A valid CRC is not evidence that timing, capacity, or another field is valid. The independent DDR4 `module_crc()` / `repair_module_crc()` functions cover bytes 128–253 with storage at 254–255, for the supported typed layouts. Vendor-profile CRCs remain unsupported.

FPM through DDR2 instead use `checksum()` / `repair_checksum()`: the low eight bits of the sum of bytes 0–62, stored at 63. This is not a balancing complement or CRC. DDR3 base CRC uses the coverage-selection bit shown above. Non-applicable domains return `UnsupportedField`; no algorithm is silently substituted. See [legacy integrity and source evidence](legacy.md).

## Typed fields and mutation

`timing()` returns exact picoseconds plus a separate optional minimum clock count. `set_timing()` rejects values that cannot be represented without rounding, preserves unchanged alternate encodings, and checks all required bytes before mutation. DDR4 signed fine offsets and DDR5 nanosecond refresh fields use different codecs; they are not interchangeable.

`manufacturing()` borrows the part-number field and returns date, serial, manufacturer, and revision bytes without guessed semantic interpretation. `set_part_number()` accepts bounded printable ASCII and pads with spaces; `set_serial_number()` writes exactly four raw bytes. Neither modifies CRC storage or surrounding bytes.

`legacy_manufacturing()` preserves the eight-byte early or two-byte DDR3 manufacturer identifier and the two-byte legacy revision without squeezing them into modern fields. Common part-number/serial setters dispatch by generation. Raw dates and identifiers are not normalized or interpreted.

Typed access requires source-backed revision/layout combinations. Unrecognized layouts remain usable through raw operations rather than being decoded under an assumed schema. See the [DDR4/DDR5 matrix](fields.md) and [legacy matrix](legacy.md) for offsets, representation limits, partial-image behavior, errors, and excluded fields. These operations do not validate a full memory configuration or make an edited image safe to program.

## External hardware interface

Rust applications implement `Read` and optionally `Write`. A successful call transfers the complete supplied buffer at the flat SPD image offset. Associated error types remain application-defined. The library does not provide a blanket hardware implementation, impose a device address, or split calls into hardware pages.

The native equivalent is `spd_io` with an opaque application context and optional read/write callbacks. `spd_io_read` or `spd_io_write` invokes the corresponding callback once, synchronously. A missing callback returns `SPD_INVALID_ARGUMENT`; either callback may be absent so read-only and write-only native integrations are possible. A nonzero callback result maps to `SPD_CALLBACK_ERROR`. Detailed bus errors remain in application-owned context. Offset-plus-length overflow is rejected before callback invocation.

Applications own:

- Hardware handles, I²C addresses, page selection, transfer limits, and hardware-specific offset mapping.
- Locking and thread safety, retries, acknowledgement polling, and write-cycle timing.
- Read-back comparison, backups, write protection, and authorization for any physical write.

A callback success means the application completed the whole transfer. Failure can leave partially changed output or partially programmed hardware. There is no rollback, automatic retry, or CRC repair. Callbacks must not retain supplied buffer pointers, throw C++ exceptions, or unwind Rust panics across the ABI. Context and buffer storage must remain valid until the call returns. The included memory-backed adapters prove the interface, not hardware compatibility.

## Native ABI version 1

See the [C/C++ FFI guide](ffi.md) for complete examples, status handling, callback integration, and binding-generation rules. The public header also carries standalone Doxygen-compatible type and function contracts.

Use `include/libspd.h` as the binding contract. `spd_abi_version()` returns `1`. Statuses are integer codes rather than Rust enums; header fields are four `uint8_t` values, and the CRC result contains two `uint16_t` values. Unknown memory-type codes remain numeric. New timing, manufacturing, editing, and module-CRC functions are additive: original layouts and signatures are unchanged. Rust core errors map to the matching native status; all scalar/structure outputs remain unchanged on validation or decoding errors.

Buffer pointers may be null only when their length is zero. Required scalar/struct output pointers and the `spd_io` pointer must be nonnull. Lengths must fit `isize::MAX`; buffers must refer to live, correctly sized memory. Mutable buffers and output objects must not overlap input buffers or each other, and all memory must have the alignment required by its C type. The caller must prevent concurrent incompatible access. The library can reject null pointers and bad lengths; it cannot validate arbitrary dangling pointers, detect invalid lifetimes, or make aliasing violations safe.

Caller memory remains caller-owned. There is no `free` API, implicit heap state, background work, or global device state. The core and native ABI intentionally do not share Rust enum layouts, slice layouts, or allocator ownership. C++ callers get status codes, not exceptions.

## Evidence and limits

See [supported fields and evidence](fields.md) and [the development plan](plan.md#reference-assessment) for exact coverage and specification/fixture gates. Current support does not implement capacity/geometry, packed DDR4 timing masks, JEP106 naming, date validation, XMP/EXPO, general module-specific decoding, semantic lint, or hardware programming. Unsupported sections are preserved as raw bytes, not silently decoded or normalized.
