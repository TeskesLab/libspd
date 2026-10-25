//! Selected, source-backed FPM/EDO/SDRAM/DDR/DDR2 SPD fields.
//!
//! Evidence: Texas Instruments [SMMU001, January 1998](https://www.ti.com/lit/ug/smmu001/smmu001.pdf),
//! appendices A (FPM/EDO), B (SDRAM), and C (raw manufacturing fields).
//! This covers the initial and revision-1 layouts, with the revision at byte 62.
//! DDR evidence: Micron MT16VDDT3264A/6464A/12864A/25664A, Rev. C, September
//! 2004, [table 22](https://mm.digikey.com/Volume0/opasdata/d220001/medias/docus/2935/MT16VDDT%203264A%2C6464A%2C12864A%2C25664A.pdf),
//! SPD revision 1.0. DDR2 evidence: Micron MT16HTF12864H(I)/25664H(I), Rev. B,
//! November 2006, [table 14](https://media.digikey.com/pdf/Data%20Sheets/Micron%20Technology%20Inc%20PDFs/MT16HTFxxx61H(I).pdf),
//! SPD revision 1.2. Their common timing and manufacturing bytes do not require
//! a module-type interpretation. Other revisions are deliberately unsupported.
//!
//! FPM/EDO expose RAS/CAS access times, not DDR tAA. SDRAM exposes minimum
//! cycle time (whole/tenths-nanosecond nibbles), tRP, tRCD, and tRAS (whole ns).
//! DDR/DDR2 expose tRP/tRCD (quarter ns) and tRAS (whole ns). Their extended
//! cycle-time fractions, maximum clock-to-data access times, and geometry are
//! not interpreted. Raw dates and manufacturer/revision bytes remain raw: TI's
//! date convention is not the same as the later Micron BCD date convention.
//!
//! All timing durations use exact integer picoseconds in the public API.
//! Unrepresentable fractions and undefined zero encodings are rejected rather
//! than rounded or treated as missing. These codecs borrow caller-owned bytes
//! and do not access hardware or validate an entire module configuration.

use crate::{Error, Timing, TimingField};

/// Raw manufacturing fields from a supported pre-DDR4 SPD layout.
///
/// The three slices borrow the original image for lifetime `'a`; other fields
/// are copied in stored order. Padding, non-ASCII bytes, and embedded NULs are
/// preserved: these slices are neither decoded UTF-8 nor NUL-terminated C
/// strings. No JEP106 bank/parity/name decoding, BCD date validation,
/// serial-number endianness, or revision convention is inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyManufacturing<'a> {
    /// Borrowed raw manufacturer identifier: eight bytes before DDR3, two for DDR3.
    pub manufacturer: &'a [u8],
    /// Raw one-byte manufacturing location, without a decoded location name.
    pub location: u8,
    /// Two raw date bytes; no year/week or BCD interpretation is imposed.
    pub date_raw: [u8; 2],
    /// Four raw serial-number bytes in stored order, not a numeric value.
    pub serial: [u8; 4],
    /// Borrowed eighteen-byte part-number field, including all stored padding.
    pub part_number: &'a [u8],
    /// Borrowed two-byte revision field, without a vendor-specific interpretation.
    pub revision: &'a [u8],
}

/// Stored and computed additive SPD checksum bytes, independent of decoding.
///
/// For supported FPM/EDO, SDRAM, DDR, and DDR2 images, `calculated` is the
/// wrapping sum of bytes `0..63`, compared with byte 63. It is not the two's
/// complement of that sum and not a CRC. A mismatch is successful decoding
/// returned as data; it does not prevent independent field access.
///
/// # Example
///
/// This synthetic SDRAM buffer demonstrates an explicit one-byte repair:
///
/// ```
/// use libspd::{Image, ImageMut};
///
/// let mut bytes = [0u8; 64];
/// bytes[2] = 0x04; // SDRAM, initial revision at byte 62 (zero).
/// bytes[9] = 0x75; // Synthetic 7.5 ns clock-period encoding.
/// let before = bytes;
/// let check = Image::new(&bytes).checksum()?;
/// assert!(!check.is_valid()); // Mismatch is Ok(check), not an error.
/// let repaired = ImageMut::new(&mut bytes).repair_checksum()?;
/// assert!(repaired.is_valid());
/// assert_eq!(&bytes[..63], &before[..63]);
/// assert_eq!(bytes[63], check.calculated);
/// assert!(Image::new(&bytes).checksum()?.is_valid());
/// # Ok::<(), libspd::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumCheck {
    /// Raw stored additive checksum byte at image offset 63.
    pub stored: u8,
    /// Sum of image bytes `0..63`, reduced modulo 256.
    pub calculated: u8,
}

impl ChecksumCheck {
    /// Return whether stored and calculated checksum bytes match.
    ///
    /// Equality checks this domain only, not timing encodings, completeness,
    /// or semantic validity of the module configuration.
    pub const fn is_valid(self) -> bool {
        self.stored == self.calculated
    }
}

pub(crate) fn validate(bytes: &[u8]) -> Result<(), Error> {
    let memory_type = *bytes.get(2).ok_or(Error::Truncated)?;
    if !matches!(memory_type, 0x01 | 0x02 | 0x04 | 0x07 | 0x08) {
        return Err(Error::UnsupportedMemoryType(memory_type));
    }
    let revision = *bytes.get(62).ok_or(Error::Truncated)?;
    let supported = match memory_type {
        0x01 | 0x02 | 0x04 => matches!(revision, 0x00 | 0x01),
        0x07 => revision == 0x10,
        0x08 => revision == 0x12,
        _ => false,
    };
    if !supported {
        return Err(Error::UnsupportedRevision(revision));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Encoding {
    Linear {
        offset: usize,
        unit: u32,
        zero_undefined: bool,
    },
    DecimalCycle,
}

impl Encoding {
    fn for_field(memory_type: u8, field: TimingField) -> Result<Self, Error> {
        use TimingField::*;
        let (offset, unit, zero_undefined) = match (memory_type, field) {
            (0x01 | 0x02, Trac) => (9, 1_000, true),
            (0x01 | 0x02, Tcac) => (10, 1_000, true),
            (0x04, TckMin) => return Ok(Self::DecimalCycle),
            (0x04, Trp) => (27, 1_000, true),
            (0x04, Trcd) => (29, 1_000, true),
            (0x04, Tras) => (30, 1_000, true),
            (0x07 | 0x08, Trp) => (27, 250, false),
            (0x07 | 0x08, Trcd) => (29, 250, false),
            (0x07 | 0x08, Tras) => (30, 1_000, false),
            _ => return Err(Error::UnsupportedField),
        };
        Ok(Self::Linear {
            offset,
            unit,
            zero_undefined,
        })
    }

    fn offset(self) -> usize {
        match self {
            Self::Linear { offset, .. } => offset,
            Self::DecimalCycle => 9,
        }
    }

    fn decode(self, raw: u8) -> Result<Timing, Error> {
        let invalid = || Error::InvalidEncoding {
            offset: self.offset(),
        };
        let picoseconds = match self {
            Self::Linear {
                unit,
                zero_undefined,
                ..
            } => {
                if raw == 0 && zero_undefined {
                    return Err(invalid());
                }
                u32::from(raw) * unit
            }
            Self::DecimalCycle => {
                let whole = raw >> 4;
                let fraction = raw & 0x0f;
                if whole == 0 || fraction > 9 {
                    return Err(invalid());
                }
                u32::from(whole) * 1_000 + u32::from(fraction) * 100
            }
        };
        Ok(Timing {
            picoseconds,
            minimum_cycles: None,
        })
    }

    fn encode(self, value: Timing) -> Result<u8, Error> {
        if value.minimum_cycles.is_some() {
            return Err(Error::ValueOutOfRange);
        }
        let ps = value.picoseconds;
        match self {
            Self::Linear {
                unit,
                zero_undefined,
                ..
            } => {
                if (ps == 0 && zero_undefined) || ps % unit != 0 || ps / unit > 255 {
                    return Err(Error::ValueOutOfRange);
                }
                Ok((ps / unit) as u8)
            }
            Self::DecimalCycle => {
                if !(1_000..=15_900).contains(&ps) || ps % 100 != 0 {
                    return Err(Error::ValueOutOfRange);
                }
                Ok(((ps / 1_000) as u8) << 4 | ((ps % 1_000) / 100) as u8)
            }
        }
    }
}

pub(crate) fn timing(bytes: &[u8], field: TimingField) -> Result<Timing, Error> {
    validate(bytes)?;
    let encoding = Encoding::for_field(bytes[2], field)?;
    let raw = *bytes.get(encoding.offset()).ok_or(Error::Truncated)?;
    encoding.decode(raw)
}

pub(crate) fn set_timing(bytes: &mut [u8], field: TimingField, value: Timing) -> Result<(), Error> {
    validate(bytes)?;
    let encoding = Encoding::for_field(bytes[2], field)?;
    let output = bytes.get_mut(encoding.offset()).ok_or(Error::Truncated)?;
    let encoded = encoding.encode(value)?;
    *output = encoded;
    Ok(())
}

pub(crate) fn manufacturing(bytes: &[u8]) -> Result<LegacyManufacturing<'_>, Error> {
    validate(bytes)?;
    let section = bytes.get(64..99).ok_or(Error::Truncated)?;
    Ok(LegacyManufacturing {
        manufacturer: &section[..8],
        location: section[8],
        date_raw: [section[29], section[30]],
        serial: [section[31], section[32], section[33], section[34]],
        part_number: &section[9..27],
        revision: &section[27..29],
    })
}

pub(crate) fn set_part_number(bytes: &mut [u8], part: &[u8]) -> Result<(), Error> {
    validate(bytes)?;
    let output = bytes.get_mut(73..91).ok_or(Error::Truncated)?;
    if part.len() > output.len() {
        return Err(Error::ValueOutOfRange);
    }
    for (index, &byte) in part.iter().enumerate() {
        if !(0x20..=0x7e).contains(&byte) {
            return Err(Error::InvalidEncoding { offset: 73 + index });
        }
    }
    output[..part.len()].copy_from_slice(part);
    output[part.len()..].fill(b' ');
    Ok(())
}

pub(crate) fn set_serial_number(bytes: &mut [u8], serial: [u8; 4]) -> Result<(), Error> {
    validate(bytes)?;
    bytes
        .get_mut(95..99)
        .ok_or(Error::Truncated)?
        .copy_from_slice(&serial);
    Ok(())
}

pub(crate) fn checksum(bytes: &[u8]) -> Result<ChecksumCheck, Error> {
    validate(bytes)?;
    let block = bytes.get(..64).ok_or(Error::Truncated)?;
    let calculated = block[..63]
        .iter()
        .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
    Ok(ChecksumCheck {
        stored: block[63],
        calculated,
    })
}

pub(crate) fn repair_checksum(bytes: &mut [u8]) -> Result<ChecksumCheck, Error> {
    let check = checksum(bytes)?;
    bytes[63] = check.calculated;
    Ok(ChecksumCheck {
        stored: check.calculated,
        calculated: check.calculated,
    })
}
