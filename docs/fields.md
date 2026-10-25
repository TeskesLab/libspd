# Supported SPD fields

This is a selected-field codec, not a complete JEDEC decoder or DIMM validator. Parsing and editing operate on caller-owned bytes; neither accesses hardware. All offsets below are decimal and ranges are inclusive unless written as Rust ranges.

This page covers DDR4/DDR5. See [legacy support](legacy.md) for FPM/EDO, SDRAM, DDR, DDR2, and DDR3, including their different revision/manufacturing locations and integrity algorithms.

## Layout and revision support

Typed timing/manufacturing access and the DDR4 module CRC require these exact headers:

| Memory type, byte 2 | Base SPD revision, byte 1 | Complete raw module-type byte, byte 3 |
|---|---|---|
| DDR4, `0x0c` | `0x11` or `0x13` | SO-DIMM, `0x03` |
| DDR5, `0x12` | `0x10` | UDIMM, `0x02` |

Other revisions and module bytes produce explicit unsupported errors. The entire module-type byte is checked: no unverified hybrid/reserved masks are inferred. This conservative matrix records inspected layouts, not a claim that other layouts necessarily differ.

Raw headers, raw reads/writes, and the original base CRC functions remain usable independently of this typed matrix. No full-image length is required: each typed operation checks only its own header/field bytes. A supported header is not a semantic-validity certificate, and a bad CRC does not prevent decoding.

## Timing API

`Image::timing(TimingField)` returns `Timing { picoseconds: u32, minimum_cycles: Option<u8> }`. `ImageMut::set_timing(field, value)` encodes the exact value or returns an error; it never rounds or silently discards a clock-count constraint. C/C++ use `spd_timing_get` and `spd_timing_set` with the corresponding field constants.

### DDR4

Byte 17 must be `0x00`, the inspected encoding for MTB = 125 ps and FTB = 1 ps. Unknown timebase encodings return `UnsupportedTimebase`, not a guessed scale. These fields have no separate clock-count member; `minimum_cycles` must be `None` when setting them.

| TimingField | Coarse byte(s) | Signed fine-offset byte | Representation |
|---|---|---|---|
| `TckMin` | 18 | 125 | Coarse × 125 ps + signed fine × 1 ps |
| `TckMax` | 19 | 124 | Same |
| `Taa` | 24 | 123 | Same |
| `Trcd` | 25 | 122 | Same |
| `Trp` | 26 | 121 | Same |
| `TrrdS` | 38 | 119 | Same |
| `TrrdL` | 39 | 118 | Same |
| `TccdL` | 40 | 117 | Same |
| `Trfc1` | 30–31 | None | Little-endian u16 × 125 ps |
| `Trfc2` | 32–33 | None | Same |
| `Trfc4` | 34–35 | None | Same |

Negative decoded durations produce `InvalidEncoding` rather than wrapping to a positive integer. Setters can replace such an invalid existing duration with a representable value. Coarse/fine setters support exact nonnegative durations through 32002 ps; new encodings use the coarse quotient plus a nonnegative fine remainder. Existing alternative encodings are not normalized when their decoded value already matches the requested value. MTB-only fields require exact multiples of 125 ps with a quotient fitting u16.

Packed-nibble timing fields (`Tras`, `Trc`, `Twr`, `Tfaw`, and `TwtrS/L`) are not enabled for DDR4 because the inspected sample tables do not provide complete bit masks. Other unlisted fields return `UnsupportedField`.

### DDR5

All duration pairs below are little-endian u16. The public API uses picoseconds consistently; nanosecond fields are scaled explicitly. A lower-clock byte is returned as `Some`, including an encoded zero. Its physical validity is not asserted.

| TimingField | Duration bytes | Stored unit | Lower-clock-limit byte (nCK) |
|---|---|---|---|
| `TckMin` | 20–21 | ps | None |
| `TckMax` | 22–23 | ps | None |
| `Taa` | 30–31 | ps | None |
| `Trcd` | 32–33 | ps | None |
| `Trp` | 34–35 | ps | None |
| `Tras` | 36–37 | ps | None |
| `Trc` | 38–39 | ps | None |
| `Twr` | 40–41 | ps | None |
| `Trfc1` | 42–43 | ns | None |
| `Trfc2` | 44–45 | ns | None |
| `TrfcSb` | 46–47 | ns | None |
| `TrrdL` | 70–71 | ps | 72 |
| `TccdL` | 73–74 | ps | 75 |
| `TccdLWr` | 76–77 | ps | 78 |
| `TccdLWr2` | 79–80 | ps | 81 |
| `Tfaw` | 82–83 | ps | 84 |
| `TwtrL` | 85–86 | ps | 87 |
| `TwtrS` | 88–89 | ps | 90 |
| `Trtp` | 91–92 | ps | 93 |

Setters require `Some(nCK)` exactly where a clock-limit byte exists and `None` elsewhere. Stored duration must fit u16; an ns field must be an exact multiple of 1000 ps. No rate-bin snapping, cycle rounding, inferred minimum cycles, or cross-field consistency repair occurs. `Trfc4` and `TrrdS` are unsupported for this DDR5 layout.

Representable zero durations and zero clock counts are preserved; representability is not configuration validation. Use an external validation policy before programming any edited image.

## Manufacturing data and edits

`Image::manufacturing()` returns raw numeric byte arrays and a borrowed fixed-width part-number slice. It does not allocate, decode dates, name a manufacturer, interpret serial endianness, or require printable text. An unknown/non-ASCII part number remains available as raw bytes.

| Field | DDR4 bytes | DDR5 bytes |
|---|---|---|
| Module manufacturer ID, raw | 320–321 | 512–513 |
| Manufacturing location, raw | 322 | 514 |
| Date, raw | 323–324 | 515–516 |
| Serial number, raw | 325–328 | 517–520 |
| Part number, fixed width | 329–348 (20 bytes) | 521–550 (30 bytes) |
| Module revision, raw | 349 | 551 |
| DRAM manufacturer ID, raw | 350–351 | 552–553 |
| DRAM stepping, raw | 352 | 554 |

The aggregate getter needs all these fields (353 bytes for DDR4, 555 for DDR5). It returns `Truncated` without preventing independent timing/header access.

`set_part_number(&[u8])` accepts printable ASCII (`0x20..=0x7e`) up to the generation’s field width and pads the remainder with spaces. Empty input explicitly clears the field to spaces. Overlength or nonprintable input is rejected before mutation. `set_serial_number([u8; 4])` writes exactly those bytes without a numeric-endian interpretation. Each setter needs only its own field to be present, not the entire manufacturing section.

All setters preserve unrelated bytes and leave checksums untouched. C/C++ getters return a fixed 30-byte part-number array with an explicit width of 20 or 30; unused array bytes are zero, not part of the image. This is not a NUL-terminated string contract.

## Additional CRC domain

For the supported DDR4 typed layouts, `module_crc()` checks bytes 128–253 against little-endian storage at 254–255, independently of `base_crc()`. `repair_module_crc()` changes only those two stored bytes and returns the post-repair check. DDR5 returns `UnsupportedField` for this operation. Neither decoding nor any setter repairs either CRC implicitly.

## Error and ABI contract

Typed errors distinguish unsupported revision/module/field/timebase, missing bytes, invalid existing encodings, and values that cannot be represented exactly. `InvalidEncoding { offset }` identifies the byte in the image or proposed field encoding. Every failed setter leaves the complete image unchanged.

The C ABI remains version 1: the changes add exports, structures, and status codes without changing any existing function signature or layout. Timing field arguments are checked u32 values, not unchecked foreign Rust enums. `spd_timing` uses a checked 0/1 presence flag and an explicitly bounded cycle count. Required output objects remain unchanged on any validation/decoding error. See `include/libspd.h` for exact sizes, signatures, statuses, pointer validity, and non-overlap requirements.

## Evidence and limits

Implementation facts were independently derived from the following publicly inspected vendor tables, not copied implementations or redistributed full fixtures:

- [Advantech SQR-SD4N8G3K2MNGEA](https://advdownload.advantech.com/productfile/Downloadfile5/1-26QU5G5/Advantech_SQR-SD4N8G3K2MNGEA.PDF), document revision 1.2, 26 May 2021, §7, PDF pages 14–17: DDR4 revision 1.3 SO-DIMM fields, coarse/fine time values, both explicit CRC spans, and manufacturing offsets.
- [Samsung M471A5244BB0-CRC](https://download.semiconductor.samsung.com/resources/data-sheet/M471A5244BB0-CRC.pdf), May 2018, pages 1–3: DDR4 revision 1.1 SO-DIMM values, including the signed fine-offset example `7 × 125 − 42 = 833 ps`, and independent CRC values.
- [Advantech SQR-UD5N16G5K6HNAG](https://advdownload.advantech.com/productfile/Downloadfile4/1-2NDX8LK/Advantech%20SQR-UD5N16G5K6HNAG%20datasheet.pdf), document revision 1.0, §10, printed pages 13–17: DDR5 revision 1.0 UDIMM pairs, units, nCK limits, CRC span, and manufacturing offsets. The cover says 2 May 2024 while revision history says 27 June 2024; page 4 also contains a contradictory SO-DIMM heading. Do not treat this sample as universal normative evidence.

The full [DDR4 contents annex](https://www.jedec.org/standards-documents/docs/spd412l-6) and [DDR5 contents standard](https://www.jedec.org/standards-documents/docs/jesd400-5d01) require JEDEC registration/login and were not acquired. The source-backed matrix therefore stays deliberately narrow. No full module image or specification is copied into the repository.

Not provided: general revision compatibility, exhaustive module types, capacity/geometry decoding, CAS masks, JEP106 bank/parity/name decoding, BCD date validation, module-specific overlays, XMP/EXPO, semantic lint, or hardware programming. Synthetic tests establish codec behavior, not real-module or all-JEDEC conformance.
