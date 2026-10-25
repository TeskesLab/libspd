//! Selected DDR3 SPD codecs, without module-configuration validation.
//!
//! Sources (public full text; no implementation code or module dump copied):
//! - Samsung M471B5673FH0-YF8/YH9/YK0, October 2009, pp. 1–2:
//!   <https://download.semiconductor.samsung.com/resources/data-sheet/555238m471b5673fh0_yf8_yh9_yk0.pdf>.
//!   Revision 1.0 reserves bytes 34–59; its timings use MTB only.
//! - Samsung M471B5273DH0-CF8/CH9/CK0/CMA, January 2011, pp. 1–2:
//!   <https://download.semiconductor.samsung.com/resources/data-sheet/717143m471b5273dh0_cf8_ch9_ck0_cma.pdf>.
//!   Revision 1.1 supplies signed fine offsets and the raw manufacturing widths.
//! - CST, "Understanding DDR3 Serial Presence Detect (SPD) Table", July 2007:
//!   <https://simmtester.com/News/PublicationArticle/153>.
//!   The byte 0 discussion identifies both CRC domains; bytes 9–11 describe
//!   dividend/divisor timebases; byte 21 assigns the tRAS/tRC high nibbles;
//!   bytes 24–29 describe little-endian tRFC and the tFAW high nibble.
//!   Bytes 117–147 describe metadata and 18-character ASCII space padding.
//!   Numerical examples are cross-checked against Samsung: the CST article
//!   has typographical errors in its alternate-timebase examples.
//!
//! Supported revisions are 1.0 and 1.1; only 1.1 uses fine offsets.
//! Only the vendor-confirmed 125 ps MTB is supported (any equivalent nonzero
//! dividend/divisor encoding). FTB uses rational nibble dividend/divisor
//! arithmetic, including Samsung's 2.5 ps and 1 ps examples. Fractional-
//! picosecond results cannot fit the public integer-picosecond API and are
//! rejected, never rounded. This is selected vendor-backed support, not a
//! claim of authoritative access to the complete JEDEC DDR3 SPD standard.
//! Module-specific bytes 60–116 are not interpreted, so byte 3 is not gated.
//! Unsuffixed DDR3 tRRD/tWTR are not silently mapped to short/long timing fields.
//!
//! The public entry points are [`crate::Image::timing`],
//! [`crate::Image::legacy_manufacturing`], and [`crate::Image::base_crc`],
//! with explicit mutation/repair counterparts on [`crate::ImageMut`].
//! Each checks only its required bytes; neither timing nor metadata access
//! requires a matching CRC or a complete module image.
//!
//! Timing counts must have a nonzero coarse component. Revision 1.0 uses
//! multiples of 125 ps; revision 1.1 fine offsets use signed eight-bit counts.
//! Fine-bearing fields can decode to zero when their exact correction permits
//! it, but negative and fractional-picosecond results are invalid. Setting
//! requires an exact representation and no separate minimum-cycle member.
//! Split-nibble writes preserve neighboring fields; setting an equal decoded
//! value preserves its original alternate coarse/fine encoding.
//!
//! Manufacturing identifiers (two bytes), part numbers (eighteen bytes), and
//! revisions (two bytes) remain borrowed raw slices. Date and serial bytes are
//! copied without BCD validation or numeric endian interpretation. No JEP106
//! bank/parity/name lookup, text trimming, or C-string interpretation occurs.
//!
//! The base CRC is CRC-16/XMODEM with little-endian storage at `126..128`.
//! Byte 0 bit 7 selects coverage: clear means `0..126`, set means `0..117`.
//! A mismatch is returned as data. Explicit repair changes only `126..128`;
//! timing and manufacturing setters never repair integrity storage implicitly.

use crate::{crc16_xmodem, CrcCheck, Error, LegacyManufacturing, Timing, TimingField};

pub(crate) fn validate(bytes: &[u8]) -> Result<(), Error> {
    let memory_type = *bytes.get(2).ok_or(Error::Truncated)?;
    if memory_type != 0x0b {
        return Err(Error::UnsupportedMemoryType(memory_type));
    }
    match bytes[1] {
        0x10 | 0x11 => Ok(()),
        revision => Err(Error::UnsupportedRevision(revision)),
    }
}

#[derive(Clone, Copy)]
enum Coarse {
    Byte(usize),
    Split { low: usize, high: usize, shift: u8 },
    Word(usize),
}

impl Coarse {
    fn offset(self) -> usize {
        match self {
            Self::Byte(offset) | Self::Word(offset) => offset,
            Self::Split { low, .. } => low,
        }
    }

    fn end(self) -> usize {
        self.offset() + if matches!(self, Self::Word(_)) { 2 } else { 1 }
    }

    fn maximum(self) -> i64 {
        match self {
            Self::Byte(_) => 255,
            Self::Split { .. } => 4095,
            Self::Word(_) => 65535,
        }
    }

    // Bounds checked by Encoding::check_bounds; split high bytes precede lows.
    fn read(self, bytes: &[u8]) -> i64 {
        match self {
            Self::Byte(offset) => i64::from(bytes[offset]),
            Self::Split { low, high, shift } => {
                i64::from(bytes[low]) + (i64::from((bytes[high] >> shift) & 15) << 8)
            }
            Self::Word(offset) => i64::from(u16::from_le_bytes([bytes[offset], bytes[offset + 1]])),
        }
    }

    fn write(self, bytes: &mut [u8], value: u16) {
        match self {
            Self::Byte(offset) => bytes[offset] = value as u8,
            Self::Split { low, high, shift } => {
                bytes[low] = value as u8;
                let mask = 15 << shift;
                bytes[high] = (bytes[high] & !mask) | (((value >> 8) as u8) << shift);
            }
            Self::Word(offset) => {
                bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Encoding {
    coarse: Coarse,
    fine: Option<usize>,
}

impl Encoding {
    fn for_field(revision: u8, field: TimingField) -> Result<Self, Error> {
        use TimingField::*;
        let (coarse, fine) = match field {
            TckMin => (Coarse::Byte(12), Some(34)),
            Taa => (Coarse::Byte(16), Some(35)),
            Twr => (Coarse::Byte(17), None),
            Trcd => (Coarse::Byte(18), Some(36)),
            Trp => (Coarse::Byte(20), Some(37)),
            Tras => (
                Coarse::Split {
                    low: 22,
                    high: 21,
                    shift: 0,
                },
                None,
            ),
            Trc => (
                Coarse::Split {
                    low: 23,
                    high: 21,
                    shift: 4,
                },
                Some(38),
            ),
            Trfc1 => (Coarse::Word(24), None),
            Trtp => (Coarse::Byte(27), None),
            Tfaw => (
                Coarse::Split {
                    low: 29,
                    high: 28,
                    shift: 0,
                },
                None,
            ),
            _ => return Err(Error::UnsupportedField),
        };
        Ok(Self {
            coarse,
            fine: if revision == 0x10 { None } else { fine },
        })
    }

    fn check_bounds(self, bytes: &[u8]) -> Result<(), Error> {
        let end = self.fine.map_or(self.coarse.end(), |offset| offset + 1);
        bytes.get(..end).ok_or(Error::Truncated)?;
        Ok(())
    }

    fn decode(self, bytes: &[u8], fine_base: (i64, i64)) -> Result<Timing, Error> {
        let coarse = self.coarse.read(bytes);
        if coarse == 0 {
            return Err(Error::InvalidEncoding {
                offset: self.coarse.offset(),
            });
        }
        let (numerator, denominator) = fine_base;
        let fine = self.fine.map_or(0, |offset| i64::from(bytes[offset] as i8));
        let scaled = coarse * 125 * denominator + fine * numerator;
        if scaled < 0 || scaled % denominator != 0 {
            return Err(Error::InvalidEncoding {
                offset: self.fine.unwrap_or(self.coarse.offset()),
            });
        }
        Ok(Timing {
            picoseconds: (scaled / denominator) as u32,
            minimum_cycles: None,
        })
    }

    fn encode(self, value: Timing, fine_base: (i64, i64)) -> Result<(u16, u8), Error> {
        if value.minimum_cycles.is_some() {
            return Err(Error::ValueOutOfRange);
        }
        let ps = i64::from(value.picoseconds);
        if self.fine.is_none() {
            if ps % 125 != 0 || !(1..=self.coarse.maximum()).contains(&(ps / 125)) {
                return Err(Error::ValueOutOfRange);
            }
            return Ok(((ps / 125) as u16, 0));
        }
        let (numerator, denominator) = fine_base;
        let target = ps * denominator;
        let step = 125 * denominator;
        // Prefer the legacy-safe rounded-up coarse duration with negative fine
        // correction. Search only the small mathematically possible interval
        // when that representation is unavailable for a particular FTB ratio.
        let preferred = ((ps + 124) / 125).clamp(1, self.coarse.maximum());
        let candidate = |coarse: i64| {
            let delta = target - coarse * step;
            if delta % numerator == 0 && (-128..=127).contains(&(delta / numerator)) {
                Some((coarse as u16, (delta / numerator) as i8 as u8))
            } else {
                None
            }
        };
        if let Some(encoded) = candidate(preferred) {
            return Ok(encoded);
        }
        let lower_target = target - 127 * numerator;
        let low = (-(-lower_target).div_euclid(step)).max(1);
        let high = ((target + 128 * numerator).div_euclid(step)).min(self.coarse.maximum());
        for coarse in low..=high {
            if coarse != preferred {
                if let Some(encoded) = candidate(coarse) {
                    return Ok(encoded);
                }
            }
        }
        Err(Error::ValueOutOfRange)
    }
}

fn timebase(bytes: &[u8], fine: bool) -> Result<(i64, i64), Error> {
    bytes.get(..12).ok_or(Error::Truncated)?;
    if bytes[10] == 0 {
        return Err(Error::InvalidEncoding { offset: 10 });
    }
    if bytes[11] == 0 {
        return Err(Error::InvalidEncoding { offset: 11 });
    }
    if u16::from(bytes[10]) * 8 != u16::from(bytes[11]) {
        return Err(Error::InvalidEncoding { offset: 10 });
    }
    if !fine {
        return Ok((1, 1));
    }
    let numerator = i64::from(bytes[9] >> 4);
    let denominator = i64::from(bytes[9] & 15);
    if numerator == 0 || denominator == 0 {
        return Err(Error::InvalidEncoding { offset: 9 });
    }
    Ok((numerator, denominator))
}

pub(crate) fn timing(bytes: &[u8], field: TimingField) -> Result<Timing, Error> {
    validate(bytes)?;
    let encoding = Encoding::for_field(bytes[1], field)?;
    encoding.check_bounds(bytes)?;
    let fine_base = timebase(bytes, encoding.fine.is_some())?;
    encoding.decode(bytes, fine_base)
}

pub(crate) fn set_timing(bytes: &mut [u8], field: TimingField, value: Timing) -> Result<(), Error> {
    validate(bytes)?;
    let encoding = Encoding::for_field(bytes[1], field)?;
    encoding.check_bounds(bytes)?;
    let fine_base = timebase(bytes, encoding.fine.is_some())?;
    // Preserve alternate exact encodings, including positive fine corrections.
    if encoding.decode(bytes, fine_base) == Ok(value) {
        return Ok(());
    }
    let (coarse, fine) = encoding.encode(value, fine_base)?;
    encoding.coarse.write(bytes, coarse);
    if let Some(offset) = encoding.fine {
        bytes[offset] = fine;
    }
    Ok(())
}

pub(crate) fn manufacturing(bytes: &[u8]) -> Result<LegacyManufacturing<'_>, Error> {
    validate(bytes)?;
    bytes.get(..148).ok_or(Error::Truncated)?;
    Ok(LegacyManufacturing {
        manufacturer: &bytes[117..119],
        location: bytes[119],
        date_raw: [bytes[120], bytes[121]],
        serial: [bytes[122], bytes[123], bytes[124], bytes[125]],
        part_number: &bytes[128..146],
        revision: &bytes[146..148],
    })
}

pub(crate) fn set_part_number(bytes: &mut [u8], part: &[u8]) -> Result<(), Error> {
    validate(bytes)?;
    bytes.get(..146).ok_or(Error::Truncated)?;
    if part.len() > 18 {
        return Err(Error::ValueOutOfRange);
    }
    if let Some(index) = part.iter().position(|byte| !(0x20..=0x7e).contains(byte)) {
        return Err(Error::InvalidEncoding {
            offset: 128 + index,
        });
    }
    bytes[128..146].fill(b' ');
    bytes[128..128 + part.len()].copy_from_slice(part);
    Ok(())
}

pub(crate) fn set_serial_number(bytes: &mut [u8], serial: [u8; 4]) -> Result<(), Error> {
    validate(bytes)?;
    bytes.get(..126).ok_or(Error::Truncated)?;
    bytes[122..126].copy_from_slice(&serial);
    Ok(())
}

pub(crate) fn base_crc(bytes: &[u8]) -> Result<CrcCheck, Error> {
    validate(bytes)?;
    bytes.get(..128).ok_or(Error::Truncated)?;
    let end = if bytes[0] & 0x80 == 0 { 126 } else { 117 };
    Ok(CrcCheck {
        stored: u16::from_le_bytes([bytes[126], bytes[127]]),
        calculated: crc16_xmodem(&bytes[..end]),
    })
}

pub(crate) fn repair_base_crc(bytes: &mut [u8]) -> Result<CrcCheck, Error> {
    let check = base_crc(bytes)?;
    bytes[126..128].copy_from_slice(&check.calculated.to_le_bytes());
    Ok(CrcCheck {
        stored: check.calculated,
        calculated: check.calculated,
    })
}
