# Development plan

## Product boundary

libspd parses and edits SPD images. It must work without hardware. Rust applications use the allocation-free `no_std` core; C and C++ applications link the separate native ABI library. External applications provide hardware access through Rust traits or C callbacks.

The library does not implement I²C/SMBus controllers, OS device access, USB/serial programmers, device discovery, SPD page selection, protection changes, PMIC control, or flashing transactions. The application maps flat SPD offsets to its hardware and owns timing, retries, locking, write permissions, and read-back verification. No operation edits real hardware implicitly.

## Architecture decisions

- One core crate, `libspd`, and one thin native crate, `libspd-ffi`; do not split schemas, CRCs, and validation into separate packages yet.
- Borrow caller-owned byte slices. No allocator or `unsafe` in the core. Decode explicit byte offsets rather than packed overlays or native bitfields.
- Retain unknown memory codes, module codes, revision bits, and bytes. Partial images may provide a header even when a CRC block is unavailable.
- Return errors for unavailable sections. Do not convert unknown data into guessed values or require a good CRC to decode a header.
- Raw edits do not repair checksums. CRC repair is an explicit operation and does not validate the meaning of fields.
- The C ABI uses fixed-width fields and integer status codes, caller-owned buffers, and synchronous callbacks. C++ uses this same header directly. No opaque heap handles or separate C++ implementation.
- ABI version 1 identifies the initial native contract. The header and implementation must evolve together; incompatible changes require an ABI-version change and release notes.

## Delivery sequence and acceptance gates

### 1. Runnable repository foundation (implemented)

Deliver project rules, this plan, workspace/build metadata, Rust/C/C++ examples, documentation, a local check command, and CI. Implement borrowed raw images, bounded reads and edits, raw four-byte identity headers, DDR4/DDR5 base CRC checking and explicit repair, and external read/write interfaces in all three languages.

Acceptance: Rust tests cover malformed/truncated data, bounds arithmetic, preservation, and CRC domains; a runnable Rust example exercises an application-owned adapter; compiled C and C++ programs link actual static/shared artifacts and exercise parsing, edits, CRC repair, callbacks, and errors. There must be no hardware driver dependency. This is not a complete SPD field decoder or semantic validator.

### 2. Evidence and fixture corpus

Acquire versioned primary SPD specifications legally and record field-level references. Add redistributable, provenance-tagged DDR4/DDR5 images across revisions, UDIMM/SODIMM, ECC, and registered/clocked layouts before claiming those layouts. Record fixture license, acquisition method, expected values, and redaction of serial numbers. Synthetic bytes are useful for bounds/CRC tests but are not hardware evidence.

Acceptance: every enabled layout has documented coverage and independently checked vectors. Never vendor confidential specifications or proprietary executables. Identify contradictory secondary sources instead of choosing silently.

Current evidence: [selected vendor-backed facts and numerical vectors](fields.md#evidence-and-limits) are available. No complete, revision-specific JEDEC contents standard or redistributable real-module corpus has been acquired. This gate remains open for general layout/conformance claims; the implementation below exposes only a conservative selected-field matrix.

### 3. Read-only typed decoding

Add independently fallible base, module-specific, timing, and manufacturing sections. Keep raw values with explicit units, revision/module dispatch, and checked capacity arithmetic. Add JEP-106 numeric identities before a sourced manufacturer-name table. Preserve unsupported fields and distinguish unavailable data from invalid encodings.

Acceptance: cross-checked field vectors, every truncation boundary, malformed-input tests/fuzzing, and parity through the C ABI. Do not claim whole-generation support from a single module fixture. Every additional layout requires its own evidence and independent vectors.

Implemented selected support: 11 DDR4 timing fields, 19 DDR5 timing fields, and raw manufacturing data, with exact revision/module dispatch and C ABI parity. [The support matrix](fields.md) lists every enabled encoding. Capacity, geometry, packed DDR4 fields, complete module-specific sections, and interpreted manufacturer/date data remain gated by further evidence. Every enabled field has deterministic truncation, representation, and preservation coverage; no completed fuzzing campaign is claimed.

Legacy support is implemented for selected FPM/EDO, SDRAM, DDR, DDR2, and DDR3 fields, with generation-aware identity, raw metadata, atomic edits, pre-DDR3 additive checksums, and DDR3 coverage-selectable CRC. [The legacy matrix](legacy.md) records exact revisions, field encodings, source limitations, and unsupported portions. This does not enable LPDDR or claim all historical SPD layouts.

### 4. Semantic checks and structured editing

Keep semantic findings separate from CRC results. Add typed setters only for verified layouts, preserving unrelated bits and rejecting unrepresentable values before mutation. Specify cross-field constraints, exact timing units, and explicit checksum policy. Start with individual fields; do not add a transaction/change-set framework without an application requirement.

Acceptance: round trips preserve every untouched bit, failed setters are atomic, and fixtures distinguish a valid CRC from a valid configuration. Publish the supported field/revision/module matrix.

Implemented selected setters: exact timing representation, bounded ASCII part numbers, and four raw serial bytes. Failed edits are atomic, no-op timing edits preserve alternate encodings, and CRC repair is always explicit. Configuration-level semantic lint and unverified bitfield setters are not enabled.

### 5. Vendor profiles and additional integrity domains

Add separately identified DDR4 module CRC, XMP 2.0/3.0, and EXPO domains only with confirmed coverage/signatures/revisions. Represent XMP/EXPO overlap explicitly. Do not repair absent or unknown profile blocks.

Acceptance: independent profile vectors, coexistence/overlap tests, exact edit previews, invalid-domain reporting, and ABI parity. Additional generations follow the same evidence gates.

Implemented additional domain: independent DDR4 module CRC check/repair for the supported typed layouts. Vendor-profile detection, decoding, editing, and CRC repair remain gated; no placeholder APIs are exposed for them.

### 6. Application integration guidance

Document integration with external backends using flat offsets and synchronous exact-buffer operations. Keep hardware-specific adapters and flashing/protection policy in consuming projects, not in libspd. Add asynchronous or chunked interfaces only for a demonstrated consumer need.

Acceptance: a consuming application can load, decode, edit, and serialize an image without a bus dependency; adapter errors, partial hardware writes, and read-back responsibility remain explicit.

## Reference assessment

The supplied `Memory-SPD-Software-Capability-Map.md` is a software survey, **not a JEDEC specification**. It supplies useful design boundaries and links, but cannot establish complete field coverage. Relevant independently accessible source snapshots include:

- [spdr CRC implementation](https://github.com/The-Open-Memory-Initiative-OMI/spdr/blob/6b0d2d55f18a87b39c260ed250f0ef23f32a53a2/spdr/src/crc.rs): DDR5 base CRC algorithm and domain described by the survey.
- [DDR5SPDEditor image layout](https://github.com/edlf/DDR5SPDEditor/blob/6c9d015098f3f25e636ff7402cf955d4cc7fa2cd/ddr5spd_structs.h): secondary layout evidence, not code to copy into this Apache-2.0 project (GPL-3.0).
- [Unified DDR SPD Flasher](https://github.com/H43TO/Unified-DDR-SPD-Flasher/tree/05a1df73ffc8a6fd22fc691e0c94950745ab919b): behavioral survey only; no dependency on its binaries, firmware, transport, or uncertain licensing.

The survey lists DDR4 base bytes 0–125 with little-endian CRC at 126–127, and DDR5 base bytes 0–509 with little-endian CRC at 510–511. Both use CRC-16/XMODEM: polynomial `0x1021`, initial value zero, no reflection, no final XOR. The initial implementation exposes these base domains only. Primary revision-specific confirmation and real-module validation remain acceptance gates for expanded schema support.

The [supported-field evidence record](fields.md#evidence-and-limits) now adds public Advantech and Samsung module tables to this secondary survey. These independently substantiate selected byte locations, timing values/units, and DDR4 module CRC vectors; they do not establish complete JEDEC conformance or the layouts excluded from the support matrix. All test images remain explicitly synthetic.
