// SPDX-License-Identifier: Apache-2.0
use super::*;
use libspd::{Timing, TimingField};

/// Exact integer-picosecond duration and optional independently stored nCK limit.
///
/// Native presence flags are exactly zero or one, not arbitrary truthy values.
/// Presence with zero cycles is distinct from absence. This is a native ABI
/// value, not serialized SPD bytes; no physical timing validity is implied.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdTiming {
    /// Duration in picoseconds, not frequency or clock cycles.
    pub picoseconds: u32,
    /// Lower nCK limit: `0..=255` when present; must be zero when absent.
    pub minimum_cycles: u32,
    /// Exactly `0` for absent or `1` for present, including present zero nCK.
    pub has_minimum_cycles: u32,
}

impl From<Timing> for SpdTiming {
    fn from(value: Timing) -> Self {
        Self {
            picoseconds: value.picoseconds,
            minimum_cycles: value.minimum_cycles.map_or(0, u32::from),
            has_minimum_cycles: u32::from(value.minimum_cycles.is_some()),
        }
    }
}

impl SpdTiming {
    fn into_core(self) -> Result<Timing, SpdStatus> {
        let minimum_cycles = match (self.has_minimum_cycles, self.minimum_cycles) {
            (0, 0) => None,
            (1, cycles) if cycles <= u8::MAX as u32 => Some(cycles as u8),
            _ => return Err(SPD_INVALID_ARGUMENT),
        };
        Ok(Timing {
            picoseconds: self.picoseconds,
            minimum_cycles,
        })
    }
}

/// Raw DDR4/DDR5 manufacturing data, separate from legacy metadata.
///
/// No manufacturer/JEP106, date, serial-endian, or text interpretation occurs.
/// Native struct padding is not image data. Use [`spd_legacy_manufacturing`]
/// for earlier formats.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdManufacturing {
    /// Two raw module-manufacturer identifier bytes.
    pub module_manufacturer: [u8; 2],
    /// One raw manufacturing-location byte.
    pub location: u8,
    /// Two raw date bytes, without BCD or calendar validation.
    pub date_raw: [u8; 2],
    /// Four raw serial bytes in image order, not an integer.
    pub serial: [u8; 4],
    /// Stored fixed-width bytes, including spaces/non-ASCII; not a C string.
    ///
    /// DDR4 returns 20 stored bytes followed by ten zero output bytes; DDR5
    /// returns all 30 stored bytes. The zero tail is not image data.
    pub part_number: [u8; 30],
    /// Stored field width (`20` for DDR4, `30` for DDR5), not trimmed text length.
    pub part_number_len: u8,
    /// One raw module-revision byte.
    pub module_revision: u8,
    /// Two raw DRAM-manufacturer identifier bytes.
    pub dram_manufacturer: [u8; 2],
    /// One raw DRAM-stepping byte.
    pub dram_stepping: u8,
}

/// Generation-aware raw identity, without typed-codec revision/module validation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdIdentityInfo {
    /// Raw type: FPM `0x01`, EDO `0x02`, SDR `0x04`, DDR `0x07`, DDR2 `0x08`,
    /// DDR3 `0x0b`, DDR4 `0x0c`, or DDR5 `0x12`.
    pub memory_type: u8,
    /// Raw SPD revision from byte 62 before DDR3, otherwise byte 1.
    pub revision: u8,
    /// Raw byte 20 for DDR2 or byte 3 for DDR3+; zero when absent.
    pub module_type: u8,
    /// Exactly `0` for absent (FPM/EDO/SDR/DDR), `1` for present, even if zero.
    pub has_module_type: u8,
}

/// Native stored/computed additive checksum bytes; compare members for equality.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdChecksumCheck {
    /// Raw checksum storage byte 63.
    pub stored: u8,
    /// Sum of bytes `0..63` modulo 256, not an additive complement.
    pub calculated: u8,
}

/// Raw pre-DDR4 manufacturing data preserving legacy field widths.
///
/// Identifiers, location, dates, serials, and revisions remain uninterpreted:
/// no JEP106 names/parity, date validation, or serial endianness is inferred.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdLegacyManufacturingInfo {
    /// Eight stored bytes before DDR3; two DDR3 bytes followed by six output zeros.
    pub manufacturer: [u8; 8],
    /// Stored identifier width: exactly `8` before DDR3 or `2` for DDR3.
    pub manufacturer_len: u8,
    /// One raw manufacturing-location byte.
    pub location: u8,
    /// Two raw date bytes with generation/vendor-dependent format.
    pub date_raw: [u8; 2],
    /// Four raw serial bytes in image order, not an integer.
    pub serial: [u8; 4],
    /// All 18 stored bytes, including padding/non-ASCII; not a C string.
    pub part_number: [u8; 18],
    /// Both raw stored revision bytes, unlike the modern one-byte revision.
    pub revision: [u8; 2],
}

/// Reads generation-aware raw identity without codec revision validation.
///
/// Requires 63 bytes before DDR3 or four for DDR3+. Unlike [`spd_header`],
/// rejects unknown memory codes; returned identity does not certify typed support.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing identity bytes, or
/// [`SPD_UNSUPPORTED_MEMORY_TYPE`] for unknown memory types.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level memory
/// contract. Errors leave the entire output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_identity(
    data: *const u8,
    len: usize,
    out: *mut SpdIdentityInfo,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; remaining readable validity is caller-owned.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.identity() {
        Ok(value) => {
            let output = SpdIdentityInfo {
                memory_type: value.memory_type.as_raw(),
                revision: value.revision,
                module_type: value.module_type.unwrap_or(0),
                has_module_type: u8::from(value.module_type.is_some()),
            };
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(output) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Copies complete raw pre-DDR4 manufacturing data; unused manufacturer tail is zero.
///
/// Requires a supported typed revision (see the crate-level matrix) and 99 bytes
/// before DDR3 or 148 for DDR3. The result keeps 18 part-number bytes and both
/// revision bytes; getters do not require printable text or valid integrity.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, [`SPD_UNSUPPORTED_MEMORY_TYPE`] for
/// unknown types, [`SPD_UNSUPPORTED_REVISION`] for unsupported revisions, or
/// [`SPD_UNSUPPORTED_FIELD`] for DDR4/DDR5.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level memory
/// contract. Errors leave the entire output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_legacy_manufacturing(
    data: *const u8,
    len: usize,
    out: *mut SpdLegacyManufacturingInfo,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; remaining readable validity is caller-owned.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.legacy_manufacturing() {
        Ok(value) => {
            let mut output = SpdLegacyManufacturingInfo {
                manufacturer: [0; 8],
                manufacturer_len: value.manufacturer.len() as u8,
                location: value.location,
                date_raw: value.date_raw,
                serial: value.serial,
                part_number: [0; 18],
                revision: [0; 2],
            };
            output.manufacturer[..value.manufacturer.len()].copy_from_slice(value.manufacturer);
            output.part_number.copy_from_slice(value.part_number);
            output.revision.copy_from_slice(value.revision);
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(output) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Checks the pre-DDR3 sum modulo 256, not an additive complement.
///
/// Requires 64 bytes and a supported FPM/EDO/SDR/DDR/DDR2 revision from the
/// crate-level matrix. Compare `stored` with `calculated`; mismatch is [`SPD_OK`].
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, [`SPD_UNSUPPORTED_MEMORY_TYPE`] for
/// unknown types, [`SPD_UNSUPPORTED_REVISION`] for unsupported legacy revisions,
/// or [`SPD_UNSUPPORTED_FIELD`] for DDR3+.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level memory
/// contract. Errors leave the entire output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_checksum(
    data: *const u8,
    len: usize,
    out: *mut SpdChecksumCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller supplies valid readable storage.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.checksum() {
        Ok(value) => {
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe {
                out.write(SpdChecksumCheck {
                    stored: value.stored,
                    calculated: value.calculated,
                })
            };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Repairs only byte 63 and returns the post-repair checksum.
///
/// Applicability and required extents match [`spd_checksum`]. Success returns
/// `stored == calculated` and preserves all other image bytes.
///
/// # Errors
/// Returns the argument, truncation, unsupported-memory/revision/field statuses
/// described by [`spd_checksum`], without modifying image or output.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level exclusive
/// memory contract. Errors leave the image and output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_repair_checksum(
    data: *mut u8,
    len: usize,
    out: *mut SpdChecksumCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller grants exclusive image access.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    match image.repair_checksum() {
        Ok(value) => {
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe {
                out.write(SpdChecksumCheck {
                    stored: value.stored,
                    calculated: value.calculated,
                })
            };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Decodes a selected timing without requiring a valid CRC.
///
/// `field` is a `u32` ID matching [`libspd::TimingField`] (1 through 23), with
/// generation-specific availability given in the crate documentation. The image
/// need only contain identification and selected timing/timebase bytes, not a
/// full image. Pre-DDR3 requires revision byte 62 even for earlier timing fields.
/// Absent cycles return flag/count zero; DDR5 nCK fields return flag one even
/// for an encoded zero count.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or unknown
/// IDs; [`SPD_TRUNCATED`] for missing bytes; unsupported memory, revision, module,
/// field, or timebase statuses for unavailable interpretations; or
/// [`SPD_INVALID_ENCODING`] for an encoding that cannot yield integer picoseconds.
///
/// # Safety
/// `data` and the required, disjoint writable `out` must obey the crate-level
/// memory contract. Errors leave `out` untouched. `field` is a numeric ID, not
/// a Rust enum; unknown IDs are rejected before decoding.
#[no_mangle]
pub unsafe extern "C" fn spd_timing_get(
    data: *const u8,
    len: usize,
    field: u32,
    out: *mut SpdTiming,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    let Some(field) = TimingField::from_raw(field) else {
        return SPD_INVALID_ARGUMENT;
    };
    // SAFETY: Null/length checked; caller supplies valid readable storage.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.timing(field) {
        Ok(value) => {
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(value.into()) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Atomically edits a selected timing, without repairing CRC or validating the
/// resulting memory configuration semantically.
///
/// IDs and required byte extents match [`spd_timing_get`]. Native flag zero
/// requires count zero; flag one requires count `0..=255`. Legacy/DDR4 and plain
/// DDR5 duration fields require absent cycles. DDR5 tRRD_L, tCCD_L, tCCD_L_WR,
/// tCCD_L_WR2, tFAW, tWTR_L, tWTR_S, and tRTP require present nCK, including zero.
/// No cycles are inferred or discarded and no duration is rounded. Unrelated
/// bits/bytes are preserved; equal decoded values retain their existing encoding.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments, unknown
/// IDs, or malformed native flags/counts. Well-formed but nonrepresentable
/// durations or inappropriate cycle presence return [`SPD_VALUE_OUT_OF_RANGE`].
/// Truncation, unsupported memory/revision/module/field/timebase, and invalid
/// encoding statuses also apply, as with [`spd_timing_get`]. Errors change nothing.
///
/// # Safety
/// `data` must obey the crate-level exclusive buffer contract. Unknown field IDs
/// and malformed presence/cycle representations are rejected without mutation.
#[no_mangle]
pub unsafe extern "C" fn spd_timing_set(
    data: *mut u8,
    len: usize,
    field: u32,
    value: SpdTiming,
) -> SpdStatus {
    if !valid_buffer(data, len) {
        return SPD_INVALID_ARGUMENT;
    }
    let Some(field) = TimingField::from_raw(field) else {
        return SPD_INVALID_ARGUMENT;
    };
    let value = match value.into_core() {
        Ok(value) => value,
        Err(status) => return status,
    };
    // SAFETY: Null/length checked; caller grants exclusive access.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    image
        .set_timing(field, value)
        .map_or_else(error_status, |()| SPD_OK)
}

/// Copies raw manufacturing fields. The part number is not a C string; its
/// image width is returned in `part_number_len`, and unused output bytes are 0.
///
/// Requires the supported typed DDR4/DDR5 layout from the crate-level matrix
/// and 353/555 accessible bytes respectively. Raw metadata is not validated or
/// interpreted; use [`spd_legacy_manufacturing`] for earlier formats.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, unsupported memory/revision/module
/// statuses for unavailable layouts, or [`SPD_UNSUPPORTED_FIELD`] before DDR4.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level memory
/// contract. Errors leave the entire output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_manufacturing_get(
    data: *const u8,
    len: usize,
    out: *mut SpdManufacturing,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; remaining readable validity is caller-owned.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.manufacturing() {
        Ok(value) => {
            let mut part_number = [0; 30];
            part_number[..value.part_number.len()].copy_from_slice(value.part_number);
            let output = SpdManufacturing {
                module_manufacturer: value.module_manufacturer,
                location: value.location,
                date_raw: value.date_raw,
                serial: value.serial,
                part_number,
                part_number_len: value.part_number.len() as u8,
                module_revision: value.module_revision,
                dram_manufacturer: value.dram_manufacturer,
                dram_stepping: value.dram_stepping,
            };
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(output) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Writes printable ASCII, right-padding the on-image part number with spaces.
/// Empty input is valid; no implicit CRC repair occurs. Errors are atomic.
///
/// Input length is explicit (no `strlen`); each byte must be `0x20..=0x7e`, so a
/// terminating NUL must not be included. Maximum widths are 18 before DDR4,
/// 20 for DDR4, and 30 for DDR5. Empty input clears the field to spaces. Success
/// needs 91 bytes before DDR3, 146 for DDR3, 349 for DDR4, or 551 for DDR5,
/// plus the format's revision identification. Other metadata need not be present.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, unsupported memory/revision/module
/// statuses for unavailable layouts, [`SPD_INVALID_ENCODING`] for nonprintable
/// input, or [`SPD_VALUE_OUT_OF_RANGE`] for text longer than the stored width.
///
/// # Safety
/// Image and input buffers must obey the crate-level memory contract and must
/// not overlap, including unused image bytes. Null input is allowed only if empty.
#[no_mangle]
pub unsafe extern "C" fn spd_part_number_set(
    data: *mut u8,
    len: usize,
    part_number: *const u8,
    part_number_len: usize,
) -> SpdStatus {
    if !valid_buffer(data, len) || !valid_buffer(part_number, part_number_len) {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Both null/length pairs checked; caller guarantees disjoint memory.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    let part_number = unsafe { bytes(part_number, part_number_len) };
    image
        .set_part_number(part_number)
        .map_or_else(error_status, |()| SPD_OK)
}

/// Writes exactly four raw serial-number bytes without repairing CRC.
///
/// No numeric-endian interpretation occurs. Success needs 99 bytes before DDR3,
/// 126 for DDR3, 329 for DDR4, or 521 for DDR5, plus revision identification.
/// Only the serial field and identification bytes are required.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, or unsupported memory/revision/module
/// statuses for unavailable typed layouts. Errors leave the image unchanged.
///
/// # Safety
/// `data` obeys the exclusive buffer contract; `serial` is required and points
/// to four live readable bytes disjoint from the entire image. Errors are atomic.
#[no_mangle]
pub unsafe extern "C" fn spd_serial_number_set(
    data: *mut u8,
    len: usize,
    serial: *const u8,
) -> SpdStatus {
    if !valid_buffer(data, len) || serial.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Required serial pointer checked; caller guarantees four bytes.
    let serial = unsafe { bytes(serial, 4) };
    let serial = [serial[0], serial[1], serial[2], serial[3]];
    // SAFETY: Null/length checked; caller grants exclusive image access.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    image
        .set_serial_number(serial)
        .map_or_else(error_status, |()| SPD_OK)
}

/// Checks the selected DDR4 module block CRC. DDR5 has no supported module CRC.
///
/// Covers `128..254` with little-endian storage at `254..256`; requires 256
/// bytes and the typed DDR4 revision/module gate. No base CRC is checked.
/// A mismatch returns [`SPD_OK`]; compare the output members.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing bytes, unsupported memory/revision/module
/// statuses for unavailable layouts, or [`SPD_UNSUPPORTED_FIELD`] for a
/// supported generation without this domain.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level memory
/// contract. Errors leave the entire output unchanged.
#[no_mangle]
pub unsafe extern "C" fn spd_module_crc(
    data: *const u8,
    len: usize,
    out: *mut SpdCrcCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller supplies valid readable storage.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.module_crc() {
        Ok(check) => {
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(check.into()) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Explicitly repairs only DDR4 module CRC bytes 254/255 and returns the valid
/// post-repair check. Errors leave image and output untouched.
///
/// Applicability and the 256-byte minimum match [`spd_module_crc`]. Neither
/// base CRC nor manufacturing bytes are changed; success returns
/// `stored == calculated`.
///
/// # Errors
/// Returns the argument, truncation, unsupported-memory/revision/module/field
/// statuses described by [`spd_module_crc`], without modifying image or output.
///
/// # Safety
/// `data` and required, disjoint writable `out` obey the crate-level exclusive
/// buffer and output contract.
#[no_mangle]
pub unsafe extern "C" fn spd_repair_module_crc(
    data: *mut u8,
    len: usize,
    out: *mut SpdCrcCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller grants exclusive image access.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    match image.repair_module_crc() {
        Ok(check) => {
            // SAFETY: Caller supplies a live, aligned, disjoint output.
            unsafe { out.write(check.into()) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}
