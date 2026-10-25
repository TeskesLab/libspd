// SPDX-License-Identifier: Apache-2.0
//! Native ABI 1 for inspecting and editing caller-owned SPD byte images.
//!
//! Rust applications normally use the safe [`libspd`] crate instead. This crate
//! implements the native boundary documented in `include/libspd.h`; it provides
//! no image handles, allocation API, hardware driver, or configuration validator.
//! Parsing, checked byte operations, and integrity calculations use the safe core.
//! All offsets and lengths are in bytes, and documented ranges are half-open.
//!
//! # Ownership, memory, and concurrency
//!
//! Callers own all buffers, output objects, callback tables, and callback state.
//! Nothing is retained after a call. Foreign callers must supply live, correctly
//! aligned objects and buffers contained in one allocation. Every buffer length
//! is at most `isize::MAX`; `len` is accessible memory, not declared SPD capacity.
//! A buffer pointer may be null only for zero length. Required scalar/struct
//! outputs and I/O tables are never optional, even for empty input. Nonempty
//! buffers must be valid for their entire declared length throughout the call.
//! The library rejects null/length/range errors, but cannot validate arbitrary
//! dangling pointers or detect overlapping storage.
//!
//! Mutable buffers/outputs must not alias any input, one another, or an [`SpdIo`]
//! object. This includes unused portions of an image buffer. All access must
//! obey Rust's shared-reference and exclusive-reference requirements. Callers
//! synchronize concurrent access to modified memory, callback state, and backend
//! storage. Independent calls on disjoint writable memory share no library state.
//! Struct members are native ABI values; struct padding is not serialized SPD data.
//!
//! # Results and mutation
//!
//! Functions return [`SPD_OK`] on success. Integrity mismatch is successful data:
//! compare the returned `stored` and `calculated` members. Decoding never requires
//! valid integrity. Unless a callback has run, errors leave images and output
//! objects untouched. Setters preserve unrelated bytes and validate encoding, not
//! physical feasibility or cross-field consistency. No setter implicitly repairs
//! integrity; explicit repair functions change only their own storage bytes.
//! Raw writes may intentionally replace any bytes, including integrity storage.
//! Unsupported statuses mean unimplemented/unverified interpretation, not proof
//! of invalid hardware.
//!
//! Normal paths use checked operations and do not panic. An unexpected Rust
//! panic cannot unwind across an `extern "C"` export: it aborts the process,
//! rather than being misreported as a caller validation error. Callbacks must
//! not panic, throw, longjmp, or retain borrowed pointers.
//!
//! # Selected typed layouts
//!
//! This is not complete JEDEC-generation coverage. Timing and manufacturing
//! codecs support FPM/EDO/SDRAM revisions `0x00`/`0x01`, DDR `0x10`, DDR2 `0x12`
//! (revision byte 62), and DDR3 `0x10`/`0x11` (revision byte 1), without a module
//! gate for these common fields. DDR4 requires revision `0x11`/`0x13` and complete
//! module byte `0x03`; DDR5 requires revision `0x10` and complete module byte
//! `0x02`. Unknown revisions/module bytes are not assumed compatible. Raw byte
//! access, literal headers, generation-aware identity, and base CRC have their
//! separately documented applicability.
//!
//! Timing IDs are checked `u32` values matching [`libspd::TimingField`], not
//! unchecked foreign Rust enums. Availability is generation-specific:
//!
//! | Format | Supported timing names |
//! | --- | --- |
//! | FPM/EDO | tRAC, tCAC |
//! | SDRAM | tCKmin, tRP, tRCD, tRAS |
//! | DDR/DDR2 | tRP, tRCD, tRAS |
//! | DDR3 | tCKmin, tAA, tWR, tRCD, tRP, tRAS, tRC, tRFC1, tRTP, tFAW |
//! | DDR4 | tCKmin, tCKmax, tAA, tRCD, tRP, tRRD_S, tRRD_L, tCCD_L, tRFC1, tRFC2, tRFC4 |
//! | DDR5 | All IDs except tRFC4, tRRD_S, tRAC, tCAC |
//!
//! DDR4 timing byte 17 must be zero (MTB 125 ps, FTB 1 ps); DDR3 validates its
//! supported rational timebase and uses fine offsets only at revision `0x11`.
//! No rounding, inferred cycle count, speed-bin snapping, or semantic validation
//! occurs. Repository examples/tests are synthetic, not real-module conformance
//! evidence. `docs/fields.md` and `docs/legacy.md` describe offsets and evidence.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_op_in_unsafe_fn)]

use libspd::{CrcCheck, Error, Image, ImageMut};
use std::ffi::c_void;
use std::slice;

/// Fixed-width signed 32-bit native status, not an enum-sized ABI parameter.
pub type SpdStatus = i32;
/// Operation succeeded; returned integrity values may still differ.
pub const SPD_OK: SpdStatus = 0;
/// Required pointer is null, length excessive, timing ID unknown, or native timing malformed.
pub const SPD_INVALID_ARGUMENT: SpdStatus = 1;
/// Image range exceeds its length, or offset plus length overflows `usize`.
pub const SPD_OUT_OF_BOUNDS: SpdStatus = 2;
/// Image lacks identification, header, field, or integrity-domain bytes required by the operation.
pub const SPD_TRUNCATED: SpdStatus = 3;
/// Memory-type byte is unsupported for the requested interpretation.
pub const SPD_UNSUPPORTED_MEMORY_TYPE: SpdStatus = 4;
/// Invoked callback returned nonzero; partial output or backend effects are possible.
pub const SPD_CALLBACK_ERROR: SpdStatus = 5;
/// Image revision is outside the operation's supported matrix.
pub const SPD_UNSUPPORTED_REVISION: SpdStatus = 6;
/// Complete raw module-type byte is outside the supported typed layout.
pub const SPD_UNSUPPORTED_MODULE_TYPE: SpdStatus = 7;
/// Known timing field or integrity/manufacturing domain is unavailable for this layout.
pub const SPD_UNSUPPORTED_FIELD: SpdStatus = 8;
/// Image or proposed encoding cannot yield the required decoded value, including invalid text.
pub const SPD_INVALID_ENCODING: SpdStatus = 9;
/// Requested value cannot be represented exactly; no rounding is performed.
pub const SPD_VALUE_OUT_OF_RANGE: SpdStatus = 10;
/// Timing timebase encoding is unsupported; no scale is guessed.
pub const SPD_UNSUPPORTED_TIMEBASE: SpdStatus = 11;

mod fields;
pub use fields::*;

/// Literal first-four-byte view, not generation-aware semantic identity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdHeader {
    /// Raw byte 0, without size/capacity interpretation.
    pub size_raw: u8,
    /// Raw byte 1, not the pre-DDR3 semantic SPD revision.
    pub revision: u8,
    /// Raw byte 2, preserving unknown memory-type codes.
    pub memory_type: u8,
    /// Raw byte 3, not pre-DDR3 semantic module identity.
    pub module_type_raw: u8,
}

/// Native stored/computed CRC-16 values; compare members for equality.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdCrcCheck {
    /// Little-endian stored image bytes decoded into a native integer.
    pub stored: u16,
    /// CRC-16/XMODEM calculated over the selected integrity domain.
    pub calculated: u16,
}

impl From<CrcCheck> for SpdCrcCheck {
    fn from(value: CrcCheck) -> Self {
        Self {
            stored: value.stored,
            calculated: value.calculated,
        }
    }
}

/// Synchronous application read callback: `(context, offset, output, len)`.
///
/// Return zero for complete success; any nonzero value maps to
/// [`SPD_CALLBACK_ERROR`] without preserving the backend code. Partial output
/// changes on failure are possible. The callback must enforce backend capacity;
/// the library checks offset overflow, not storage bounds.
///
/// # Safety
/// The invoking caller supplies a writable destination for all `len` bytes under
/// the crate-level memory contract. Zero length permits null `output`. `context`
/// may be null; the application defines its interpretation and ensures valid
/// state and synchronization. The callback must honor destination exclusivity,
/// must not access overlapping table/context storage incompatibly, and must not
/// retain pointers, unwind, throw, or longjmp. Buffer access lasts only until
/// return. The callback is invoked once even for an empty validated request.
pub type SpdReadCallback = unsafe extern "C" fn(*mut c_void, usize, *mut u8, usize) -> i32;
/// Synchronous application write callback: `(context, offset, input, len)`.
///
/// Return zero for complete success; any nonzero value maps to
/// [`SPD_CALLBACK_ERROR`] without preserving the backend code. Partial backend
/// writes on failure are possible. The callback enforces backend capacity.
///
/// # Safety
/// The invoking caller supplies readable input for all `len` bytes under the
/// crate-level memory contract. Zero length permits null `input`. `context` may
/// be null; the application defines its interpretation and ensures valid state
/// and synchronization. The callback must not mutate the input or access
/// overlapping context storage incompatibly. It must not retain pointers,
/// unwind, throw, or longjmp. Buffer access lasts only until return. The callback
/// is invoked once even for an empty validated request.
pub type SpdWriteCallback = unsafe extern "C" fn(*mut c_void, usize, *const u8, usize) -> i32;

/// Caller-owned callback table with independently optional read/write directions.
///
/// Context/backend lifetimes and synchronization belong to the application.
/// The library supplies no discovery, retries, splitting, paging, bus access,
/// timing, or protection management.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SpdIo {
    /// Opaque borrowed state passed unchanged to callbacks; null is allowed.
    pub context: *mut c_void,
    /// Read callback; `None` makes [`spd_io_read`] return [`SPD_INVALID_ARGUMENT`].
    pub read: Option<SpdReadCallback>,
    /// Write callback; `None` makes [`spd_io_write`] return [`SPD_INVALID_ARGUMENT`].
    pub write: Option<SpdWriteCallback>,
}

fn error_status(error: Error) -> SpdStatus {
    match error {
        Error::OutOfBounds => SPD_OUT_OF_BOUNDS,
        Error::Truncated => SPD_TRUNCATED,
        Error::UnsupportedMemoryType(_) => SPD_UNSUPPORTED_MEMORY_TYPE,
        Error::UnsupportedRevision(_) => SPD_UNSUPPORTED_REVISION,
        Error::UnsupportedModuleType(_) => SPD_UNSUPPORTED_MODULE_TYPE,
        Error::UnsupportedField => SPD_UNSUPPORTED_FIELD,
        Error::InvalidEncoding { .. } => SPD_INVALID_ENCODING,
        Error::ValueOutOfRange => SPD_VALUE_OUT_OF_RANGE,
        Error::UnsupportedTimebase(_) => SPD_UNSUPPORTED_TIMEBASE,
    }
}

fn valid_buffer(pointer: *const u8, len: usize) -> bool {
    len <= isize::MAX as usize && (len == 0 || !pointer.is_null())
}

fn valid_range(total: usize, offset: usize, len: usize) -> bool {
    matches!(offset.checked_add(len), Some(end) if end <= total)
}

// SAFETY: Callers validate null/length before calling these helpers and uphold
// the module-level allocation, lifetime and non-aliasing preconditions. Empty
// slices need no dereference and must not be built from a possibly null pointer.
unsafe fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        // SAFETY: See the helper's preconditions above.
        unsafe { slice::from_raw_parts(pointer, len) }
    }
}

unsafe fn bytes_mut<'a>(pointer: *mut u8, len: usize) -> &'a mut [u8] {
    if len == 0 {
        &mut []
    } else {
        // SAFETY: See the helper's preconditions above; the caller grants
        // exclusive access to this complete buffer for the returned lifetime.
        unsafe { slice::from_raw_parts_mut(pointer, len) }
    }
}

/// Returns the native ABI version, always `1` for this interface.
#[no_mangle]
pub extern "C" fn spd_abi_version() -> u32 {
    1
}

/// Reads the first four bytes, preserving unknown memory-type codes.
///
/// Success requires at least four accessible bytes. Use [`spd_identity`] for
/// generation-aware revision/module slots; no codec validation occurs here.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or
/// [`SPD_TRUNCATED`] for fewer than four bytes.
///
/// # Safety
/// `data` and `out` must obey the module-level memory contract. `out` is a
/// required writable `SpdHeader` and must not overlap `data`. Errors leave it
/// untouched; even a truncated image must be valid for its declared length.
#[no_mangle]
pub unsafe extern "C" fn spd_header(data: *const u8, len: usize, out: *mut SpdHeader) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Validated null/length; remaining validity is caller-owned.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.header() {
        Ok(header) => {
            let value = SpdHeader {
                size_raw: header.size_raw,
                revision: header.revision,
                memory_type: header.memory_type.as_raw(),
                module_type_raw: header.module_type_raw,
            };
            // SAFETY: The caller supplies a disjoint writable output.
            unsafe { out.write(value) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Copies a checked image range; no partial copy on validation/range error.
///
/// Copies exactly `output_len` bytes without parsing or integrity validation.
/// A zero-length copy still requires `offset <= len`.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or
/// [`SPD_OUT_OF_BOUNDS`] if `offset + output_len` overflows or exceeds `len`.
///
/// # Safety
/// `data` and `output` must obey the module-level memory contract. Their entire
/// buffers must be disjoint, not merely the range being copied.
#[no_mangle]
pub unsafe extern "C" fn spd_image_read(
    data: *const u8,
    len: usize,
    offset: usize,
    output: *mut u8,
    output_len: usize,
) -> SpdStatus {
    if !valid_buffer(data, len) || !valid_buffer(output, output_len) {
        return SPD_INVALID_ARGUMENT;
    }
    if !valid_range(len, offset, output_len) {
        return SPD_OUT_OF_BOUNDS;
    }
    // SAFETY: Null/length/range checked; caller guarantees valid, disjoint
    // shared input and exclusive output allocations for this call.
    let image = Image::new(unsafe { bytes(data, len) });
    let output = unsafe { bytes_mut(output, output_len) };
    image
        .read(offset, output)
        .map_or_else(error_status, |()| SPD_OK)
}

/// Edits raw bytes without implicit CRC repair. Range errors are atomic.
///
/// Replaces exactly `input_len` bytes, including integrity storage if selected,
/// without parsing. A zero-length write still requires `offset <= len`.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or
/// [`SPD_OUT_OF_BOUNDS`] if `offset + input_len` overflows or exceeds `len`.
///
/// # Safety
/// `data` must grant exclusive access, and `input` shared access, under the
/// module-level memory contract. The entire buffers must not overlap.
#[no_mangle]
pub unsafe extern "C" fn spd_image_write(
    data: *mut u8,
    len: usize,
    offset: usize,
    input: *const u8,
    input_len: usize,
) -> SpdStatus {
    if !valid_buffer(data, len) || !valid_buffer(input, input_len) {
        return SPD_INVALID_ARGUMENT;
    }
    if !valid_range(len, offset, input_len) {
        return SPD_OUT_OF_BOUNDS;
    }
    // SAFETY: Null/length/range checked; caller provides valid, disjoint
    // exclusive image and shared input buffers.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    let input = unsafe { bytes(input, input_len) };
    image
        .write(offset, input)
        .map_or_else(error_status, |()| SPD_OK)
}

/// Checks DDR3/DDR4/DDR5 base-block CRC; mismatch itself is not an error.
///
/// DDR3 requires 128 bytes and revision `0x10`/`0x11`: byte 0 bit 7 set covers
/// `0..117`, clear covers `0..126`; storage is `126..128`. DDR4 covers `0..126`
/// with storage `126..128`; DDR5 covers `0..510` with storage `510..512`.
/// Stored pairs are little-endian. DDR4/DDR5 impose no typed revision/module
/// gate here. This checks no other integrity domain.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments,
/// [`SPD_TRUNCATED`] for missing required bytes, [`SPD_UNSUPPORTED_MEMORY_TYPE`]
/// for unknown types, [`SPD_UNSUPPORTED_REVISION`] for unsupported DDR3 revisions,
/// or [`SPD_UNSUPPORTED_FIELD`] for pre-DDR3. A mismatch returns [`SPD_OK`].
///
/// # Safety
/// `data` and the required, disjoint writable `out` must obey the module-level
/// memory contract. On error `out` is untouched.
#[no_mangle]
pub unsafe extern "C" fn spd_base_crc(
    data: *const u8,
    len: usize,
    out: *mut SpdCrcCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller guarantees readable memory.
    let image = Image::new(unsafe { bytes(data, len) });
    match image.base_crc() {
        Ok(check) => {
            // SAFETY: Caller guarantees valid, disjoint output memory.
            unsafe { out.write(check.into()) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Explicitly repairs only the stored base CRC, returning its post-repair value.
///
/// Applicability and required extents match [`spd_base_crc`]. Only its two
/// storage bytes change; DDR3 coverage selection is preserved. Success returns
/// `stored == calculated`.
///
/// # Errors
/// Returns the argument, truncation, unsupported-memory/revision/field statuses
/// described by [`spd_base_crc`], without modifying image or output.
///
/// # Safety
/// `data` must be exclusively writable and `out` a required writable object,
/// disjoint under the module-level memory contract. Errors leave both untouched.
#[no_mangle]
pub unsafe extern "C" fn spd_repair_base_crc(
    data: *mut u8,
    len: usize,
    out: *mut SpdCrcCheck,
) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller guarantees exclusive image memory.
    let mut image = ImageMut::new(unsafe { bytes_mut(data, len) });
    match image.repair_base_crc() {
        Ok(check) => {
            // SAFETY: Caller guarantees valid, disjoint output memory.
            unsafe { out.write(check.into()) };
            SPD_OK
        }
        Err(error) => error_status(error),
    }
}

/// Computes standalone CRC-16/XMODEM, including zero for empty input.
///
/// Uses polynomial `0x1021`, initial value zero, no reflection, and no final
/// XOR. This accepts arbitrary bytes rather than selecting an SPD domain.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments.
///
/// # Safety
/// `data` and required, disjoint writable `out` must obey the module-level memory
/// contract. Validation errors leave `out` untouched.
#[no_mangle]
pub unsafe extern "C" fn spd_crc16_xmodem(data: *const u8, len: usize, out: *mut u16) -> SpdStatus {
    if !valid_buffer(data, len) || out.is_null() {
        return SPD_INVALID_ARGUMENT;
    }
    // SAFETY: Null/length checked; caller guarantees readable data.
    let value = libspd::crc16_xmodem(unsafe { bytes(data, len) });
    // SAFETY: Caller guarantees a valid, disjoint scalar output.
    unsafe { out.write(value) };
    SPD_OK
}

/// Calls the application's read callback once, synchronously, without retries.
///
/// Invokes a present callback even for zero length. The callback enforces backend
/// capacity; validation failures invoke no callback and leave output untouched.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or a
/// missing read callback, [`SPD_OUT_OF_BOUNDS`] for `offset + len` overflow, or
/// [`SPD_CALLBACK_ERROR`] for any nonzero callback result. No rollback occurs.
///
/// # Safety
/// `io` must be a live aligned `SpdIo` with a valid function pointer or `None`;
/// `output` obeys the module-level memory contract and cannot overlap `io` or
/// context memory accessed incompatibly by the callback. The callback must
/// honor the buffer/lifetime and must not unwind, throw or retain its pointer.
/// A callback error may follow partial output modification; no rollback occurs.
#[no_mangle]
pub unsafe extern "C" fn spd_io_read(
    io: *const SpdIo,
    offset: usize,
    output: *mut u8,
    len: usize,
) -> SpdStatus {
    if io.is_null() || !valid_buffer(output, len) {
        return SPD_INVALID_ARGUMENT;
    }
    if offset.checked_add(len).is_none() {
        return SPD_OUT_OF_BOUNDS;
    }
    // SAFETY: The caller supplies a valid aligned io object.
    let io = unsafe { &*io };
    let Some(callback) = io.read else {
        return SPD_INVALID_ARGUMENT;
    };
    // SAFETY: Validated buffer/offset; callback validity and its adherence
    // to the documented lifetime, exclusivity and no-unwind contract are
    // the caller's responsibility. No Rust slice is held over the call.
    if unsafe { callback(io.context, offset, output, len) } == 0 {
        SPD_OK
    } else {
        SPD_CALLBACK_ERROR
    }
}

/// Calls the application's write callback once, synchronously, without retries.
///
/// Invokes a present callback even for zero length. The callback enforces backend
/// capacity; validation failures invoke no backend operation.
///
/// # Errors
/// Returns [`SPD_INVALID_ARGUMENT`] for invalid null/length arguments or a
/// missing write callback, [`SPD_OUT_OF_BOUNDS`] for `offset + len` overflow, or
/// [`SPD_CALLBACK_ERROR`] for any nonzero callback result. No rollback occurs.
///
/// # Safety
/// `io` must be a live aligned `SpdIo` with a valid function pointer or `None`;
/// `input` obeys the module-level memory contract and cannot alias context memory
/// being mutated by the callback. The callback must honor the buffer/lifetime
/// and must not unwind, throw or retain its pointer. A callback error may follow
/// partial storage/hardware writes; the library cannot roll them back.
#[no_mangle]
pub unsafe extern "C" fn spd_io_write(
    io: *const SpdIo,
    offset: usize,
    input: *const u8,
    len: usize,
) -> SpdStatus {
    if io.is_null() || !valid_buffer(input, len) {
        return SPD_INVALID_ARGUMENT;
    }
    if offset.checked_add(len).is_none() {
        return SPD_OUT_OF_BOUNDS;
    }
    // SAFETY: The caller supplies a valid aligned io object.
    let io = unsafe { &*io };
    let Some(callback) = io.write else {
        return SPD_INVALID_ARGUMENT;
    };
    // SAFETY: Validated buffer/offset; caller supplies the callback and
    // guarantees its validity and conformance to the documented contract.
    if unsafe { callback(io.context, offset, input, len) } == 0 {
        SPD_OK
    } else {
        SPD_CALLBACK_ERROR
    }
}

#[cfg(test)]
mod tests;
