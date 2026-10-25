//! Borrowed, allocation-free access to raw Serial Presence Detect (SPD) images.
//!
//! This `no_std` crate operates on caller-owned byte slices: it does not allocate,
//! discover devices, access a bus, or program hardware. Images may be empty or
//! partial; each operation checks the bytes it needs, not a declared image size.
//! All offsets and lengths are in bytes, and ranges in these docs are half-open.
//!
//! # Find an operation
//!
//! - **Raw access and identification:** [`Image::new`], [`Image::as_bytes`],
//!   [`Image::read`], [`ImageMut::write`], [`Image::memory_type`],
//!   [`Image::header`], and [`Image::identity`].
//! - **Selected timings:** [`Image::timing`], [`ImageMut::set_timing`],
//!   [`TimingField`], and [`Timing`]. Durations are exact integer picoseconds;
//!   separately stored minimum clock counts are not inferred or rounded.
//! - **Raw manufacturing data:** [`Image::manufacturing`] and [`Manufacturing`]
//!   for DDR4/DDR5; [`Image::legacy_manufacturing`] and [`LegacyManufacturing`]
//!   for earlier formats. Edit with [`ImageMut::set_part_number`] and
//!   [`ImageMut::set_serial_number`].
//! - **Independent integrity domains:** [`Image::checksum`] /
//!   [`ImageMut::repair_checksum`] for pre-DDR3, [`Image::base_crc`] /
//!   [`ImageMut::repair_base_crc`] for DDR3/DDR4/DDR5, and [`Image::module_crc`] /
//!   [`ImageMut::repair_module_crc`] for the supported DDR4 layouts.
//!   [`crc16_xmodem`] also computes a CRC without interpreting an SPD image.
//! - **Application-supplied I/O:** [`Read`] and [`Write`] describe synchronous
//!   whole-buffer access. Implementations own addressing, paging, protection,
//!   retries, synchronization, and their hardware or storage backend.
//!
//! # Ownership, interpretation, and integrity
//!
//! [`Image`] is a copyable shared borrow, not a parsed or owned copy.
//! [`ImageMut`] holds an exclusive borrow; its read-only views and borrowed
//! manufacturing fields cannot outlive the corresponding borrow. No image is
//! normalized on construction. Unknown fields, padding, and trailers remain
//! available unchanged through raw access.
//!
//! A [`Header`] is literally bytes 0 through 3. On pre-DDR3 images its
//! `revision` and `module_type_raw` slots are **not** semantic revision/module
//! identity: use [`Identity`] via [`Image::identity`] instead. Even a recognized
//! identity does not establish support for typed decoding.
//!
//! Decoding never requires valid integrity data. A checksum/CRC mismatch is a
//! successful check with unequal stored/calculated values, not an [`Error`].
//! Setters preserve unrelated bytes and are atomic on error, but never repair
//! integrity implicitly. Repair each applicable domain explicitly after editing;
//! repairing a CRC is not validation that a configuration is safe to program.
//! Raw writes can deliberately replace any bytes, including integrity storage.
//!
//! Manufacturing data is raw, not a manufacturer-name or text-decoding service.
//! Modern manufacturer IDs have two bytes, part numbers have 20 bytes for DDR4
//! or 30 for DDR5, and module revisions have one byte. Legacy manufacturer IDs
//! have eight bytes before DDR3 or two for DDR3; part numbers have 18 bytes and
//! revisions have two. Both result types retain two date bytes and four serial
//! bytes without date validation or serial-endian interpretation. Part-number
//! getters preserve padding and arbitrary bytes: they are not NUL-terminated
//! strings. Setters accept bounded printable ASCII and right-pad with spaces.
//!
//! # Deliberately limited field coverage
//!
//! This is a selected-field codec, not complete generation coverage or a DIMM
//! validator. Typed DDR4 access supports revision bytes `0x11`/`0x13` with the
//! complete SO-DIMM module byte `0x03`; DDR5 supports revision `0x10` with UDIMM
//! byte `0x02`. Selected legacy codecs support FPM/EDO/SDRAM revisions
//! `0x00`/`0x01`, DDR `0x10`, DDR2 `0x12`, and DDR3 `0x10`/`0x11`.
//! Individual timing fields and timebases remain format-specific; see
//! [`TimingField`] and [`Image::timing`]. Raw access and identification are not
//! subject to this typed-layout matrix. Integrity domains have their own gates,
//! documented on their methods.
//!
//! Capacity, packed geometry, CAS masks, general revision compatibility,
//! exhaustive module types, JEP106 names/bank/parity decoding, BCD date
//! validation, vendor profiles (XMP/EXPO), and cross-field/physical feasibility
//! validation are not provided. Synthetic examples demonstrate codec behavior,
//! not conformance of a real module or approval to write one.
//!
//! # Quickstart: inspect, edit, and explicitly repair
//!
//! These synthetic bytes contain only a supported DDR5 header, a selected
//! timing, and base-CRC space; they are not a complete valid module description.
//!
//! ```
//! use libspd::{Error, Image, ImageMut, MemoryType, Timing, TimingField};
//!
//! let mut bytes = [0u8; 512];
//! bytes[1] = 0x10; // Supported DDR5 SPD revision.
//! bytes[2] = 0x12; // DDR5.
//! bytes[3] = 0x02; // Supported UDIMM layout.
//! bytes[20..22].copy_from_slice(&625u16.to_le_bytes()); // tCK minimum, ps.
//!
//! let image = Image::new(&bytes);
//! assert_eq!(image.identity()?.memory_type, MemoryType::Ddr5);
//! assert_eq!(image.timing(TimingField::TckMin)?.picoseconds, 625);
//! // A timing can be read before the CRC block is available.
//! assert_eq!(Image::new(&bytes[..22]).timing(TimingField::TckMin)?,
//!            Timing { picoseconds: 625, minimum_cycles: None });
//! assert_eq!(Image::new(&bytes[..22]).base_crc(), Err(Error::Truncated));
//!
//! {
//!     let mut image = ImageMut::new(&mut bytes);
//!     let original_crc = image.repair_base_crc()?.stored;
//!     assert!(image.as_image().base_crc()?.is_valid());
//!     image.set_timing(TimingField::TckMin,
//!                      Timing { picoseconds: 624, minimum_cycles: None })?;
//!     assert_eq!(image.as_image().timing(TimingField::TckMin)?.picoseconds, 624);
//!     let stale = image.as_image().base_crc()?;
//!     assert_eq!(stale.stored, original_crc); // The setter did not repair it.
//!     assert!(!stale.is_valid());
//!     let repaired = image.repair_base_crc()?;
//!     assert!(repaired.is_valid());
//!     assert_eq!(image.as_image().base_crc()?, repaired);
//! }
//! // The mutable borrow ended; the caller still owns the edited storage.
//! assert_eq!(&bytes[20..22], &624u16.to_le_bytes());
//! assert!(Image::new(&bytes).base_crc()?.is_valid());
//! # Ok::<(), libspd::Error>(())
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use core::fmt;

mod ddr3;
mod integrity;
mod legacy;
mod manufacturing;
mod timing;

pub use legacy::{ChecksumCheck, LegacyManufacturing};
pub use manufacturing::Manufacturing;
pub use timing::{Timing, TimingField};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Ddr4,
    Ddr5,
}

/// An error accessing or interpreting the supported part of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A byte range exceeds the buffer, or its end overflows `usize`.
    OutOfBounds,
    /// The image lacks bytes needed for the requested section or field.
    Truncated,
    /// The requested interpretation is not supported for this memory-type code.
    UnsupportedMemoryType(
        /// Unmodified memory-type discriminator from image byte 2.
        u8,
    ),
    /// The requested codec does not support this on-image SPD revision.
    UnsupportedRevision(
        /// Raw revision byte: byte 62 before DDR3, otherwise byte 1.
        u8,
    ),
    /// Typed access is not supported for this complete raw module-type byte.
    UnsupportedModuleType(
        /// Unmasked byte 3 of the DDR4/DDR5 image, including reserved/hybrid bits.
        u8,
    ),
    /// The requested field/domain is not supported for this layout.
    UnsupportedField,
    /// An existing or proposed field encoding cannot be interpreted as supported.
    ///
    /// Examples include malformed timebase ratios, negative or fractional-ps
    /// decoded durations, and nonprintable proposed part-number bytes.
    InvalidEncoding {
        /// Zero-based image-byte offset identifying the invalid encoding.
        ///
        /// For a proposed part number this is its destination offset in the
        /// image, not an index relative to the setter's input slice.
        offset: usize,
    },
    /// A requested value cannot be represented exactly by the target field.
    ValueOutOfRange,
    /// The DDR4 timing-base encoding is not supported.
    UnsupportedTimebase(
        /// Unmodified DDR4 timebase byte at image offset 17; only zero is supported.
        u8,
    ),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds => f.write_str("byte range is out of bounds"),
            Self::Truncated => f.write_str("image is truncated for this operation"),
            Self::UnsupportedMemoryType(raw) => {
                write!(f, "unsupported memory type 0x{raw:02x}")
            }
            Self::UnsupportedRevision(raw) => {
                write!(f, "unsupported SPD revision 0x{raw:02x}")
            }
            Self::UnsupportedModuleType(raw) => {
                write!(f, "unsupported module type 0x{raw:02x}")
            }
            Self::UnsupportedField => f.write_str("field is not supported for this layout"),
            Self::InvalidEncoding { offset } => {
                write!(f, "invalid field encoding at byte {offset}")
            }
            Self::ValueOutOfRange => f.write_str("value cannot be represented exactly"),
            Self::UnsupportedTimebase(raw) => {
                write!(f, "unsupported DDR4 timebase 0x{raw:02x}")
            }
        }
    }
}

impl core::error::Error for Error {}

/// The raw header's memory-type discriminator, not full format validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// Fast page mode DRAM; raw code `0x01`.
    Fpm,
    /// Extended data-out DRAM; raw code `0x02`.
    Edo,
    /// Synchronous DRAM; raw code `0x04`.
    Sdram,
    /// DDR SDRAM; raw code `0x07`.
    Ddr,
    /// DDR2 SDRAM; raw code `0x08`.
    Ddr2,
    /// DDR3 SDRAM; raw code `0x0b`.
    Ddr3,
    /// DDR4 SDRAM; raw code `0x0c`.
    Ddr4,
    /// DDR5 SDRAM; raw code `0x12`.
    Ddr5,
    /// An unrecognized code, preserved without interpretation.
    Unknown(
        /// Original byte value; classification does not reject unknown codes.
        u8,
    ),
}

impl MemoryType {
    /// Classify a raw memory-type byte, preserving unrecognized values.
    ///
    /// Classification alone does not validate revisions, layouts, or integrity.
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0x01 => Self::Fpm,
            0x02 => Self::Edo,
            0x04 => Self::Sdram,
            0x07 => Self::Ddr,
            0x08 => Self::Ddr2,
            0x0b => Self::Ddr3,
            0x0c => Self::Ddr4,
            0x12 => Self::Ddr5,
            other => Self::Unknown(other),
        }
    }

    /// Return the raw memory-type byte.
    ///
    /// Recognized codes round-trip through [`Self::from_raw`]; an
    /// [`Self::Unknown`] value returns its payload unchanged.
    pub const fn as_raw(self) -> u8 {
        match self {
            Self::Fpm => 0x01,
            Self::Edo => 0x02,
            Self::Sdram => 0x04,
            Self::Ddr => 0x07,
            Self::Ddr2 => 0x08,
            Self::Ddr3 => 0x0b,
            Self::Ddr4 => 0x0c,
            Self::Ddr5 => 0x12,
            Self::Unknown(raw) => raw,
        }
    }
}

/// The first four header bytes; reserved bits and unknown values are retained.
///
/// These fields do not establish that an image is complete, CRC-valid, or
/// otherwise conformant to a JEDEC specification.
/// `revision` is raw byte 1 and `module_type_raw` is raw byte 3. On pre-DDR3
/// images these slots are not the semantic revision or module type; use
/// [`Image::identity`] for generation-aware identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Raw byte 0, without interpreting capacity or the declared image length.
    pub size_raw: u8,
    /// Raw byte 1, which is not the pre-DDR3 revision.
    pub revision: u8,
    /// Classification of raw byte 2, preserving unknown discriminator values.
    pub memory_type: MemoryType,
    /// Raw byte 3, which is not the pre-DDR3 module type.
    pub module_type_raw: u8,
}

/// Generation-aware raw identification, without layout or integrity validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Memory-type discriminator from byte 2; recognized by [`Image::identity`].
    pub memory_type: MemoryType,
    /// Raw semantic revision: byte 62 before DDR3, otherwise byte 1.
    pub revision: u8,
    /// Raw module-type byte: byte 20 for DDR2 or byte 3 for DDR3 and later.
    ///
    /// `None` for FPM/EDO/SDRAM/DDR; `Some(0)` is a present field containing zero,
    /// not absence. No module-specific interpretation or supported-layout
    /// validation is implied.
    pub module_type: Option<u8>,
}

/// Stored and computed values for a selected CRC domain.
///
/// Returned by base-CRC and DDR4 module-CRC operations. A mismatch is data,
/// not a decoding error; equality does not validate other domains or fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrcCheck {
    /// Stored little-endian CRC bytes decoded as an integer.
    pub stored: u16,
    /// CRC-16/XMODEM computed over the operation's documented byte domain.
    pub calculated: u16,
}

impl CrcCheck {
    /// Whether the stored value equals the calculated value.
    ///
    /// This says nothing about other integrity domains or configuration validity.
    pub const fn is_valid(self) -> bool {
        self.stored == self.calculated
    }
}

/// A borrowed raw image of any length, including an empty or partial image.
///
/// Construction does not parse, allocate, copy, validate, or access hardware.
/// Copying this handle shares the same immutable storage. Typed methods require
/// only their own identification/field bytes, not a complete image or valid CRC.
#[derive(Debug, Clone, Copy)]
pub struct Image<'a> {
    bytes: &'a [u8],
}

impl<'a> Image<'a> {
    /// Borrow bytes without validating or changing them.
    ///
    /// Empty and partial images are accepted. The caller retains ownership and
    /// cannot mutate the underlying bytes while this shared borrow is in use.
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Return the exact original bytes, including unknown fields and trailers.
    ///
    /// The slice retains the original input lifetime `'a`, rather than being
    /// limited to the borrow of this handle. No copy or interpretation occurs.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Read the literal first four bytes without format validation.
    ///
    /// No complete-image or CRC validation is performed. Unknown memory types
    /// are returned as [`MemoryType::Unknown`], not rejected. The byte-1 and
    /// byte-3 slots are not pre-DDR3 semantic revision/module identity; use
    /// [`Self::identity`] for those.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than four bytes are present.
    pub fn header(&self) -> Result<Header, Error> {
        let bytes = self.bytes.get(..4).ok_or(Error::Truncated)?;
        Ok(Header {
            size_raw: bytes[0],
            revision: bytes[1],
            memory_type: MemoryType::from_raw(bytes[2]),
            module_type_raw: bytes[3],
        })
    }
    /// Classify byte 2 without requiring the rest of a header.
    ///
    /// Unknown codes are returned as [`MemoryType::Unknown`]. This operation
    /// neither checks revision/layout support nor validates integrity.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if fewer than three bytes are present.
    pub fn memory_type(&self) -> Result<MemoryType, Error> {
        self.bytes
            .get(2)
            .copied()
            .map(MemoryType::from_raw)
            .ok_or(Error::Truncated)
    }

    /// Read semantic revision and module-type slots for this generation.
    ///
    /// Pre-DDR3 revision is byte 62, not raw header byte 1. Module type is
    /// absent for FPM/EDO/SDRAM/DDR and stored at byte 20 for DDR2.
    /// DDR3 and later use bytes 1 and 3. Values remain raw and are not a
    /// declaration that the codec supports the revision or module layout.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] when byte 2 or a generation's identity slot
    /// is missing (63 bytes are needed before DDR3; four for DDR3 and later).
    /// Unknown memory codes return [`Error::UnsupportedMemoryType`]. Recognized
    /// but unsupported revisions/module bytes are returned unchanged, not rejected.
    pub fn identity(&self) -> Result<Identity, Error> {
        let memory_type = self.memory_type()?;
        let (revision_offset, module_offset) = match memory_type {
            MemoryType::Fpm | MemoryType::Edo | MemoryType::Sdram | MemoryType::Ddr => (62, None),
            MemoryType::Ddr2 => (62, Some(20)),
            MemoryType::Ddr3 | MemoryType::Ddr4 | MemoryType::Ddr5 => (1, Some(3)),
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        };
        let revision = *self.bytes.get(revision_offset).ok_or(Error::Truncated)?;
        let module_type = match module_offset {
            Some(offset) => Some(*self.bytes.get(offset).ok_or(Error::Truncated)?),
            None => None,
        };
        Ok(Identity {
            memory_type,
            revision,
            module_type,
        })
    }

    // Typed codecs are deliberately limited to the source-backed layout matrix.
    // Raw access and the existing base CRC API remain independent of this gate.
    pub(crate) fn layout(&self) -> Result<Layout, Error> {
        let header = self.header()?;
        let (layout, module_type) = match header.memory_type {
            MemoryType::Ddr4 => {
                if !matches!(header.revision, 0x11 | 0x13) {
                    return Err(Error::UnsupportedRevision(header.revision));
                }
                (Layout::Ddr4, 0x03)
            }
            MemoryType::Ddr5 => {
                if header.revision != 0x10 {
                    return Err(Error::UnsupportedRevision(header.revision));
                }
                (Layout::Ddr5, 0x02)
            }
            memory_type => return Err(Error::UnsupportedMemoryType(memory_type.as_raw())),
        };
        if header.module_type_raw != module_type {
            return Err(Error::UnsupportedModuleType(header.module_type_raw));
        }
        Ok(layout)
    }

    /// Copy a bounded byte range without interpreting it.
    ///
    /// A zero-length read is allowed at any offset up to the image length.
    /// Invalid or overflowing ranges leave `output` unchanged.
    /// Success fills all of `output`; no allocation or partial copy occurs.
    /// The source remains borrowed and unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfBounds`] if `offset + output.len()` overflows or
    /// exceeds the image length, including an empty read past the end.
    pub fn read(&self, offset: usize, output: &mut [u8]) -> Result<(), Error> {
        let end = offset.checked_add(output.len()).ok_or(Error::OutOfBounds)?;
        let input = self.bytes.get(offset..end).ok_or(Error::OutOfBounds)?;
        output.copy_from_slice(input);
        Ok(())
    }

    /// Check only the supported base CRC, not all image integrity fields.
    ///
    /// DDR3 covers `0..117` when byte 0 bit 7 is set, otherwise `0..126`;
    /// storage is little-endian at `126..128`. Pre-DDR3 uses [`Self::checksum`].
    /// DDR4 covers bytes `0..126`, with little-endian storage at `126..128`.
    /// DDR5 covers bytes `0..510`, with storage at `510..512`. Additional bytes
    /// are ignored, and a full module image is not required. In particular this
    /// does not check the DDR4 module-specific block CRC.
    ///
    /// A mismatch returns `Ok` with unequal values. DDR4/DDR5 revision and
    /// module-type bytes are not gated here; DDR3 requires revision `0x10` or
    /// `0x11`. No bytes are changed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for fewer than four bytes or a missing
    /// required CRC block (128 bytes for DDR3/DDR4, 512 for DDR5).
    /// Pre-DDR3 formats return [`Error::UnsupportedField`]; unrecognized
    /// memory types return [`Error::UnsupportedMemoryType`]. DDR3 can return
    /// [`Error::UnsupportedRevision`] independently of CRC contents.
    pub fn base_crc(&self) -> Result<CrcCheck, Error> {
        if self.bytes.len() < 4 {
            return Err(Error::Truncated);
        }
        let offset = match self.memory_type()? {
            MemoryType::Ddr3 => return ddr3::base_crc(self.bytes),
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => return Err(Error::UnsupportedField),
            MemoryType::Ddr4 => 126,
            MemoryType::Ddr5 => 510,
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        };
        let block = self.bytes.get(..offset + 2).ok_or(Error::Truncated)?;
        Ok(CrcCheck {
            stored: u16::from_le_bytes([block[offset], block[offset + 1]]),
            calculated: crc16_xmodem(&block[..offset]),
        })
    }

    /// Check the pre-DDR3 additive byte checksum independently of typed fields.
    ///
    /// Bytes `0..63` are summed modulo 256 and compared with byte 63.
    /// This is not an additive complement and not a CRC.
    /// Requires one of the supported legacy revisions listed in the crate docs,
    /// but no module-type interpretation. A mismatch returns `Ok` with unequal
    /// values. Bytes after offset 63 are ignored and no bytes are changed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if identification or the 64-byte checksum
    /// block is missing, [`Error::UnsupportedRevision`] for an unsupported
    /// pre-DDR3 revision, [`Error::UnsupportedField`] for DDR3/DDR4/DDR5, or
    /// [`Error::UnsupportedMemoryType`] for an unknown discriminator.
    pub fn checksum(&self) -> Result<ChecksumCheck, Error> {
        match self.memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => legacy::checksum(self.bytes),
            MemoryType::Ddr3 | MemoryType::Ddr4 | MemoryType::Ddr5 => Err(Error::UnsupportedField),
            MemoryType::Unknown(raw) => Err(Error::UnsupportedMemoryType(raw)),
        }
    }
}

/// A mutable borrowed image; edits never implicitly update CRC storage.
///
/// The caller owns the storage and lends it exclusively for `'a`. Construction
/// accepts any length without validation or copying. All image setters check
/// their full operation before mutation and leave the complete image unchanged
/// on error; this is not a promise about external [`Write`] implementations.
/// Parsing and edits never access hardware or automatically repair integrity.
#[derive(Debug)]
pub struct ImageMut<'a> {
    bytes: &'a mut [u8],
}

impl<'a> ImageMut<'a> {
    /// Borrow bytes without validating, normalizing, or changing them.
    ///
    /// The exclusive borrow prevents other access to the caller's slice while
    /// this handle or a derived view is in use. Empty and partial slices are valid.
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }

    /// Borrow a read-only view for the duration of this borrow.
    ///
    /// This reborrows the same storage without copying. The view and any data
    /// borrowed through it must cease being used before the next mutable
    /// operation on this handle; they do not inherit the full lifetime `'a`.
    ///
    /// ```
    /// use libspd::{ImageMut, MemoryType};
    ///
    /// let mut bytes = [0, 0x10, 0x12, 0x02];
    /// {
    ///     let mut image = ImageMut::new(&mut bytes);
    ///     {
    ///         let view = image.as_image();
    ///         assert_eq!(view.header()?.memory_type, MemoryType::Ddr5);
    ///         assert_eq!(view.as_bytes(), &[0, 0x10, 0x12, 0x02]);
    ///     } // End the shared view before editing through the exclusive borrow.
    ///     image.write(1, &[0x11])?;
    ///     assert_eq!(image.as_image().header()?.revision, 0x11);
    /// }
    /// assert_eq!(bytes, [0, 0x11, 0x12, 0x02]);
    /// # Ok::<(), libspd::Error>(())
    /// ```
    pub fn as_image(&self) -> Image<'_> {
        Image::new(self.bytes)
    }

    /// Replace exactly the requested bytes, preserving all other bytes and CRCs.
    ///
    /// The entire range is checked before mutation, including addition overflow.
    /// On error no bytes change. Empty writes are allowed through the end offset.
    /// No revision, field, or encoding validation occurs; the input may replace
    /// integrity storage too. This copies from a separate shared slice under
    /// Rust's borrowing rules, not an overlapping in-place move.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfBounds`] if `offset + input.len()` overflows or
    /// exceeds the image length, including an empty write past the end.
    pub fn write(&mut self, offset: usize, input: &[u8]) -> Result<(), Error> {
        let end = offset.checked_add(input.len()).ok_or(Error::OutOfBounds)?;
        let output = self.bytes.get_mut(offset..end).ok_or(Error::OutOfBounds)?;
        output.copy_from_slice(input);
        Ok(())
    }

    /// Explicitly repair only the supported base CRC's two storage bytes.
    ///
    /// Coverage and partial-image behavior are those of [`Image::base_crc`].
    /// All checks precede mutation; errors leave the image untouched. On success
    /// the returned check describes the repaired image (`stored == calculated`).
    /// DDR3's byte-0 coverage selector is preserved. No module CRC, checksum,
    /// unrelated byte, or trailer is changed; no hardware is written.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for missing discriminator/CRC bytes,
    /// [`Error::UnsupportedField`] for pre-DDR3 formats,
    /// [`Error::UnsupportedMemoryType`] for unknown memory codes, or
    /// [`Error::UnsupportedRevision`] for an unsupported DDR3 revision.
    /// Required extents and applicability otherwise match [`Image::base_crc`].
    pub fn repair_base_crc(&mut self) -> Result<CrcCheck, Error> {
        if self.as_image().memory_type()? == MemoryType::Ddr3 {
            return ddr3::repair_base_crc(self.bytes);
        }
        let check = self.as_image().base_crc()?;
        let offset = match self.bytes[2] {
            0x0c => 126,
            0x12 => 510,
            // Successful base_crc establishes one of these formats.
            raw => return Err(Error::UnsupportedMemoryType(raw)),
        };
        self.bytes[offset..offset + 2].copy_from_slice(&check.calculated.to_le_bytes());
        Ok(CrcCheck {
            stored: check.calculated,
            calculated: check.calculated,
        })
    }

    /// Explicitly repair only the pre-DDR3 additive checksum storage byte.
    ///
    /// Uses the same supported revisions, 64-byte extent, and byte-sum
    /// algorithm as [`Image::checksum`]. All checks precede mutation. Success
    /// changes only byte 63 and returns equal stored/calculated values; other
    /// integrity domains and all remaining bytes are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for missing identification/checksum bytes,
    /// [`Error::UnsupportedRevision`] for an unsupported pre-DDR3 revision,
    /// [`Error::UnsupportedField`] for DDR3/DDR4/DDR5, or
    /// [`Error::UnsupportedMemoryType`] for unknown memory codes. Errors leave
    /// the entire image unchanged.
    pub fn repair_checksum(&mut self) -> Result<ChecksumCheck, Error> {
        match self.as_image().memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => legacy::repair_checksum(self.bytes),
            MemoryType::Ddr3 | MemoryType::Ddr4 | MemoryType::Ddr5 => Err(Error::UnsupportedField),
            MemoryType::Unknown(raw) => Err(Error::UnsupportedMemoryType(raw)),
        }
    }
}

/// CRC-16/XMODEM of arbitrary bytes, independent of SPD interpretation.
///
/// Polynomial `0x1021`, initial value zero, no reflection and no final XOR.
/// The ASCII input `123456789` produces `0x31c3`.
///
/// This function accepts empty input (returning zero), allocates nothing, and
/// does not select an SPD coverage range or read/write stored CRC bytes.
///
/// ```
/// assert_eq!(libspd::crc16_xmodem(b"123456789"), 0x31c3);
/// assert_eq!(libspd::crc16_xmodem(&[]), 0);
/// ```
pub fn crc16_xmodem(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Application-supplied synchronous access using flat byte offsets.
///
/// This is an adapter contract, not an implementation or automatic I/O layer.
/// The application/implementor owns backend capacity and overflow checks,
/// device addressing, paging, timing, retries, synchronization, and any bus
/// driver. The crate does not discover a backend, split requests, or supply
/// rollback. Implementations need not use callbacks.
///
/// Success means the entire requested buffer was filled, not a short read.
/// An error may leave partial output and backend effects; unlike [`Image::read`],
/// the trait makes no atomicity guarantee.
pub trait Read {
    /// Backend-defined failure type, shared by [`Write`] for this adapter.
    ///
    /// No conversion to [`crate::Error`], rollback, or retry is supplied.
    type Error;

    /// Fill exactly `output.len()` bytes from the backend at byte `offset`.
    ///
    /// `Ok(())` promises that every requested byte was read, including the
    /// zero-byte case; a short read must be handled internally or returned as
    /// an error. The output is exclusively borrowed for this call and the
    /// adapter must honor its full length. This operation does not interpret
    /// an SPD image or check integrity.
    ///
    /// The implementor defines capacity, zero-length boundary behavior,
    /// offset-overflow handling, and backend side effects. Callers must obey
    /// the chosen backend's access requirements and provide synchronization
    /// across adapters that reach the same device/storage.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] under the backend's error policy. Failure may
    /// follow partially modified `output` or backend state; no rollback is
    /// guaranteed. Callers must not treat any output bytes as a complete image
    /// after failure without a backend-specific guarantee.
    fn read(&mut self, offset: usize, output: &mut [u8]) -> Result<(), Self::Error>;
}

/// Application-supplied synchronous writes, independent of CRC handling.
///
/// Extends [`Read`] with the same backend error type and caller-owned
/// addressing, capacity/overflow checks, paging, timing, retries, write
/// protection, and synchronization responsibilities. Success means the whole
/// input was written, not a short write. Unlike [`ImageMut::write`], errors can
/// follow partial backend changes; this trait cannot promise atomicity.
///
/// The crate supplies no automatic checksum/CRC repair, persistence policy,
/// verification, rollback, or hardware access. Preparing an edited image and
/// deciding whether it is safe to program remain application responsibilities.
pub trait Write: Read {
    /// Write exactly `input.len()` bytes to the backend at byte `offset`.
    ///
    /// `Ok(())` promises the complete request was written, including the
    /// zero-byte case, under the backend's completion/durability contract; it
    /// does not itself promise power-loss persistence or read-back validation.
    /// Short writes must be completed internally or returned as an error.
    /// `input` is borrowed for this call and must not be modified.
    ///
    /// The implementor defines capacity, zero-length boundary behavior,
    /// offset-overflow handling, and backend effects. Callers must arrange
    /// applicable protection, synchronization, and device-access conditions;
    /// the trait does not retry, split requests, or repair image integrity.
    ///
    /// # Errors
    ///
    /// Returns [`Read::Error`] under the backend's error policy. Some bytes may
    /// already have reached hardware/storage on failure; no rollback is
    /// guaranteed. Recovery and any subsequent read-back are caller policy.
    fn write(&mut self, offset: usize, input: &[u8]) -> Result<(), Self::Error>;
}
