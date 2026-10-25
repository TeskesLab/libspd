# Legacy SPD support

Selected codecs cover FPM, EDO, SDRAM, DDR, DDR2, and DDR3. These are image operations only, not hardware drivers or complete module/configuration validators. Examples and regression inputs are synthetic, not captured module dumps.

## Identification and supported revisions

`Image::memory_type()` classifies byte 2 with only three input bytes. `Image::identity()` returns the generation-aware revision and optional raw module type, even if the revision is unsupported for typed decoding. C/C++ use `spd_identity`.

| Memory type | Byte 2 | Revision location | Typed codec revisions | Identity module-type location |
|---|---|---|---|---|
| FPM DRAM | `0x01` | 62 | `0x00`, `0x01` | Not present |
| EDO DRAM | `0x02` | 62 | `0x00`, `0x01` | Not present |
| SDRAM | `0x04` | 62 | `0x00`, `0x01` | Not present |
| DDR SDRAM | `0x07` | 62 | `0x10` | Not exposed |
| DDR2 SDRAM | `0x08` | 62 | `0x12` | 20 |
| DDR3 SDRAM | `0x0b` | 1 | `0x10`, `0x11` | 3 |

The existing `header()` / `spd_header` contract remains a raw first-four-byte view. **Do not treat its byte-1 `revision` slot as a legacy SPD revision or its byte-3 `module_type_raw` slot as a legacy module type.** For early formats these bytes describe capacity and row addressing. Use `identity()` for semantic identification. Absence of a module-type field is distinct from a present field whose raw value is zero.

Legacy common-field codecs do not interpret module-specific blocks or infer a module type from row count. Unknown revisions and unavailable fields return errors; raw byte access remains available. DDR4/DDR5 retain their [existing support matrix](fields.md).

## Selected timing decoding and editing

Use the existing `timing()` / `set_timing()` APIs, or `spd_timing_get` / `spd_timing_set`. Values use exact integer picoseconds. These formats do not expose separate nCK constraints in the selected fields; setting `minimum_cycles: Some(_)` is rejected.

| Format | Supported fields | Representation |
|---|---|---|
| FPM, EDO | `Trac`, `Tcac` | Bytes 9/10: RAS/CAS access time, whole ns |
| SDRAM | `TckMin` | Byte 9: whole/tenths-ns nibbles, not a binary integer |
| SDRAM | `Trp`, `Trcd`, `Tras` | Bytes 27/29/30: whole ns |
| DDR, DDR2 | `Trp`, `Trcd` | Bytes 27/29: quarter ns |
| DDR, DDR2 | `Tras` | Byte 30: whole ns |
| DDR3 | `TckMin`, `Taa`, `Twr`, `Trcd`, `Trp`, `Tras`, `Trc`, `Trfc1`, `Trtp`, `Tfaw` | MTB counts, selected packed/LE fields, and signed fine corrections where defined |

`Trac` and `Tcac` have public numeric IDs 22 and 23, respectively. They are access times, not synonyms for DDR CAS latency or a minimum clock period. DDR/DDR2 extended fractional cycle-time encodings and unverified legacy clock-to-data access encodings are not enabled.

Undefined zero values in the enabled early TI timing encodings and malformed SDRAM decimal nibbles are rejected. Setters require exact unit divisibility, field range, and valid representations. They never silently round or alter surrounding bytes/checksums.

### DDR3 timebases and packed fields

The codec supports the inspected 125 ps MTB, checking bytes 10/11 as a nonzero dividend/divisor ratio rather than assuming a scale. Where a fine offset is used, byte 9 supplies a nonzero rational FTB. Signed corrections retain that ratio; fractional-picosecond results cannot fit the public integer-picosecond API and return an error instead of rounding.

Revision 1.0 reserves the fine-offset bytes: they are ignored and preserved. Revision 1.1 uses the verified signed offsets. No-op setters preserve alternate exact encodings. Setters own only their part of shared timing nibbles, preserve adjacent fields, and can replace a malformed old duration with a representable new value.

DDR3 tRRD/tWTR are not silently mapped to the differently named DDR4 short/long fields. Unsupported timing IDs for a format remain explicit errors. No capacity, CAS-mask, geometry, speed-bin, or cross-field semantic validation is implied.

## Manufacturing bytes

Use `legacy_manufacturing()` / `spd_legacy_manufacturing` for pre-DDR4 layouts. This separate result preserves the eight-byte early manufacturer field and two-byte revision field instead of discarding bytes to fit the DDR4/DDR5 result. The existing modern `manufacturing()` result stays unchanged.

| Raw field | FPM through DDR2 | DDR3 |
|---|---|---|
| Manufacturer ID | 64–71 | 117–118 |
| Location | 72 | 119 |
| Date | 93–94 | 120–121 |
| Serial | 95–98 | 122–125 |
| Part number | 73–90 | 128–145 |
| Revision | 91–92 | 146–147 |

All identifiers/dates/revisions remain raw: the older TI and later vendor date conventions differ. Rust borrows variable-width fields; native output has an explicit manufacturer length and zeroes unused output tail bytes. Part numbers have 18 bytes and are not NUL-terminated strings.

The existing `set_part_number()` and `set_serial_number()` functions dispatch to these layouts. Part numbers accept bounded printable ASCII, right-pad with spaces, and permit empty input to clear the field. Serial edits replace exactly four raw bytes. All errors leave the complete image unchanged. Metadata getters need the complete selected metadata section; setters need only their own fields plus the format/revision identification bytes.

## Integrity is generation-specific

| Formats | Operation | Covered bytes | Storage |
|---|---|---|---|
| FPM through DDR2 | `checksum()` | 0–62 | Byte 63: low eight bits of the byte sum |
| DDR3, byte 0 bit 7 set | `base_crc()` | 0–116 | 126–127, little-endian CRC-16/XMODEM |
| DDR3, byte 0 bit 7 clear | `base_crc()` | 0–125 | 126–127, little-endian CRC-16/XMODEM |

The early checksum is **not** a two’s-complement balancing byte and is **not** CRC-16. Its repair API changes only byte 63. DDR3 CRC repair changes only bytes 126–127 and must preserve the coverage-selection bit. Both operations are explicit; parsing and edits never repair integrity data automatically. Calling a non-applicable domain returns `UnsupportedField` rather than choosing another algorithm silently.

Native additions remain ABI 1: `spd_identity`, `spd_legacy_manufacturing`, `spd_checksum`, and `spd_repair_checksum`. Existing timing/edit/base-CRC exports handle the newly supported formats through the Rust core; no second native decoder exists.

## Evidence and limits

- [TI SMMU001, January 1998](https://www.ti.com/lit/ug/smmu001/smmu001.pdf), appendices A–C: FPM/EDO and SDRAM definitions, revision at 62, additive checksum, raw manufacturing layout.
- [Micron MT16VDDT3264A/6464A/12864A/25664A](https://mm.digikey.com/Volume0/opasdata/d220001/medias/docus/2935/MT16VDDT%203264A%2C6464A%2C12864A%2C25664A.pdf), Rev. C, September 2004, table 22, vendor-authored document on distributor DigiKey: DDR revision 1.0 sample table.
- [Micron MT16HTF12864H(I)/25664H(I)](https://media.digikey.com/pdf/Data%20Sheets/Micron%20Technology%20Inc%20PDFs/MT16HTFxxx61H(I).pdf), Rev. B, November 2006, table 14, distributor-hosted vendor document: DDR2 revision 1.2 sample table.
- [Samsung DDR3, October 2009](https://download.semiconductor.samsung.com/resources/data-sheet/555238m471b5673fh0_yf8_yh9_yk0.pdf), pp. 1–2: revision 1.0 and reserved fine bytes; [Samsung DDR3, January 2011](https://download.semiconductor.samsung.com/resources/data-sheet/717143m471b5273dh0_cf8_ch9_ck0_cma.pdf), pp. 1–2: revision 1.1, fine-offset values, manufacturing fields.
- [CST, Understanding DDR3 SPD, July 2007](https://simmtester.com/News/PublicationArticle/153): explanatory field definitions for CRC selection, rational timebases, and packed timing nibbles, corroborated with Samsung numerical examples. This tester-vendor article is not a normative JEDEC standard. Its numerical examples contain apparent errors; those examples are not used as authoritative vectors.

The official [DDR annex](https://www.jedec.org/standards-documents/docs/spd-4010204), [DDR2 annex](https://www.jedec.org/standards-documents/docs/spd-4010210), and [DDR3 annex](https://www.jedec.org/standards-documents/docs/spd-4010211) remain separate acquisition targets. No unofficial mirror is treated as authenticated JEDEC access or as sole evidence for broader revision support. No specification or full vendor dump is redistributed. Selected codec support is not complete all-module/all-revision conformance or real-hardware validation.
