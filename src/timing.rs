//! Selected timing codecs with exact integer-picosecond values.
//!
//! Support is field- and generation-specific, not a complete timing model.

use crate::{ddr3, legacy, Error, Image, ImageMut, Layout, MemoryType};

/// A timing selector shared by the supported generations.
///
/// Each variant describes a timing meaning, not a promise of support on every
/// generation or revision. [`Image::timing`] rejects unsupported combinations
/// instead of inferring missing timings. The supported field/offset matrices
/// are documented in the repository's `docs/fields.md` and `docs/legacy.md`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimingField {
    /// Minimum clock period; supported for SDRAM and DDR3/DDR4/DDR5, not DDR/DDR2.
    TckMin = 1,
    /// Maximum clock period; supported only for the selected DDR4/DDR5 layouts.
    TckMax = 2,
    /// Minimum CAS access latency (tAA); supported for DDR3/DDR4/DDR5.
    /// This is not the legacy maximum clock-to-data access time.
    Taa = 3,
    /// Minimum row-to-column (RAS-to-CAS) delay; supported from SDRAM through DDR5.
    Trcd = 4,
    /// Minimum row-precharge time; supported from SDRAM through DDR5.
    Trp = 5,
    /// Minimum active-to-precharge time; supported for SDRAM, DDR, DDR2, DDR3,
    /// and DDR5. The packed DDR4 encoding is not supported.
    Tras = 6,
    /// Minimum row-cycle time; supported for DDR3 and DDR5, not packed DDR4.
    Trc = 7,
    /// Minimum write-recovery time; supported for DDR3 and DDR5, not packed DDR4.
    Twr = 8,
    /// Refresh recovery time: DDR3 tRFC, or DDR4/DDR5 tRFC1.
    /// Earlier generations' refresh timings are not decoded here.
    Trfc1 = 9,
    /// tRFC2 refresh recovery time; supported only for DDR4/DDR5.
    Trfc2 = 10,
    /// tRFC4 refresh recovery time; supported only for DDR4, not DDR5.
    Trfc4 = 11,
    /// Same-bank refresh recovery time (tRFCsb); supported only for DDR5.
    TrfcSb = 12,
    /// Short activate-to-activate delay (different bank groups); DDR4 only.
    /// Unsuffixed DDR3 tRRD is not mapped to this selector.
    TrrdS = 13,
    /// Long activate-to-activate delay (same bank group); DDR4/DDR5 only.
    /// Unsuffixed DDR3 tRRD is not mapped to this selector.
    TrrdL = 14,
    /// Long CAS-to-CAS delay (same bank group); DDR4/DDR5 only.
    TccdL = 15,
    /// Long write-to-write CAS delay (tCCD_L_WR); DDR5 only.
    TccdLWr = 16,
    /// Separate DDR5 tCCD_L_WR2 write-to-write constraint.
    /// No equivalent is inferred for earlier generations.
    TccdLWr2 = 17,
    /// Four-activate-window time; DDR3 and DDR5 only, not packed DDR4.
    Tfaw = 18,
    /// Short write-to-read delay (different bank groups); DDR5 only.
    /// Unsuffixed DDR3 tWTR is not mapped to this selector.
    TwtrS = 19,
    /// Long write-to-read delay (same bank group); DDR5 only.
    /// Unsuffixed DDR3 tWTR is not mapped to this selector.
    TwtrL = 20,
    /// Read-to-precharge delay; DDR3 and DDR5 only.
    Trtp = 21,
    /// Row-address (RAS) access time; supported only for asynchronous FPM/EDO.
    Trac = 22,
    /// Column-address (CAS) access time; supported only for asynchronous FPM/EDO.
    Tcac = 23,
}

impl TimingField {
    /// Decode a stable public field identifier, returning `None` for unknown values.
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            1 => Some(Self::TckMin),
            2 => Some(Self::TckMax),
            3 => Some(Self::Taa),
            4 => Some(Self::Trcd),
            5 => Some(Self::Trp),
            6 => Some(Self::Tras),
            7 => Some(Self::Trc),
            8 => Some(Self::Twr),
            9 => Some(Self::Trfc1),
            10 => Some(Self::Trfc2),
            11 => Some(Self::Trfc4),
            12 => Some(Self::TrfcSb),
            13 => Some(Self::TrrdS),
            14 => Some(Self::TrrdL),
            15 => Some(Self::TccdL),
            16 => Some(Self::TccdLWr),
            17 => Some(Self::TccdLWr2),
            18 => Some(Self::Tfaw),
            19 => Some(Self::TwtrS),
            20 => Some(Self::TwtrL),
            21 => Some(Self::Trtp),
            22 => Some(Self::Trac),
            23 => Some(Self::Tcac),
            _ => None,
        }
    }

    /// Return the stable public field identifier.
    pub const fn as_raw(self) -> u32 {
        self as u32
    }
}

/// An exact duration and, where stored separately, its minimum clock count.
///
/// The two constraints remain independent; no clock frequency is assumed and no
/// semantic module-configuration validation is performed.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Timing {
    /// Exact duration in picoseconds (1 ps = 10⁻¹² seconds).
    ///
    /// No floating-point approximation or frequency conversion is involved.
    /// Setters reject values that the selected field cannot encode exactly;
    /// representable zero does not imply a physically valid configuration.
    pub picoseconds: u32,
    /// Independently stored minimum clock count, in nCK (clock cycles).
    ///
    /// `None` means the selected encoding has no separate count byte, not that
    /// its count is zero. `Some(0)` preserves a present, zero-valued count.
    /// Only selected DDR5 fields store this constraint; setters require presence
    /// to match the field exactly and never derive cycles from the duration.
    pub minimum_cycles: Option<u8>,
}

#[derive(Clone, Copy)]
enum Encoding {
    Ddr4Fine {
        coarse: usize,
        fine: usize,
    },
    Word {
        offset: usize,
        unit: u32,
        cycles: Option<usize>,
    },
}

impl Encoding {
    fn for_field(layout: Layout, field: TimingField) -> Result<Self, Error> {
        use TimingField::*;
        match layout {
            Layout::Ddr4 => {
                let (coarse, fine) = match field {
                    TckMin => (18, 125),
                    TckMax => (19, 124),
                    Taa => (24, 123),
                    Trcd => (25, 122),
                    Trp => (26, 121),
                    TrrdS => (38, 119),
                    TrrdL => (39, 118),
                    TccdL => (40, 117),
                    Trfc1 | Trfc2 | Trfc4 => {
                        let offset = match field {
                            Trfc1 => 30,
                            Trfc2 => 32,
                            _ => 34,
                        };
                        return Ok(Self::Word {
                            offset,
                            unit: 125,
                            cycles: None,
                        });
                    }
                    _ => return Err(Error::UnsupportedField),
                };
                Ok(Self::Ddr4Fine { coarse, fine })
            }
            Layout::Ddr5 => {
                let (offset, unit, cycles) = match field {
                    TckMin => (20, 1, None),
                    TckMax => (22, 1, None),
                    Taa => (30, 1, None),
                    Trcd => (32, 1, None),
                    Trp => (34, 1, None),
                    Tras => (36, 1, None),
                    Trc => (38, 1, None),
                    Twr => (40, 1, None),
                    Trfc1 => (42, 1_000, None),
                    Trfc2 => (44, 1_000, None),
                    TrfcSb => (46, 1_000, None),
                    TrrdL => (70, 1, Some(72)),
                    TccdL => (73, 1, Some(75)),
                    TccdLWr => (76, 1, Some(78)),
                    TccdLWr2 => (79, 1, Some(81)),
                    Tfaw => (82, 1, Some(84)),
                    TwtrL => (85, 1, Some(87)),
                    TwtrS => (88, 1, Some(90)),
                    Trtp => (91, 1, Some(93)),
                    _ => return Err(Error::UnsupportedField),
                };
                Ok(Self::Word {
                    offset,
                    unit,
                    cycles,
                })
            }
        }
    }

    fn required_end(self) -> usize {
        match self {
            Self::Ddr4Fine { fine, .. } => fine + 1,
            Self::Word { offset, cycles, .. } => cycles.map_or(offset + 2, |offset| offset + 1),
        }
    }

    // The caller has checked required_end and the DDR4 timebase.
    fn decode(self, bytes: &[u8]) -> Result<Timing, Error> {
        match self {
            Self::Ddr4Fine { coarse, fine } => {
                let signed = i32::from(bytes[coarse]) * 125 + i32::from(bytes[fine] as i8);
                let picoseconds =
                    u32::try_from(signed).map_err(|_| Error::InvalidEncoding { offset: fine })?;
                Ok(Timing {
                    picoseconds,
                    minimum_cycles: None,
                })
            }
            Self::Word {
                offset,
                unit,
                cycles,
            } => Ok(Timing {
                picoseconds: u32::from(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
                    * unit,
                minimum_cycles: cycles.map(|offset| bytes[offset]),
            }),
        }
    }

    // Produce every replacement byte before the caller mutates the image.
    fn encode(self, value: Timing) -> Result<[u8; 3], Error> {
        match self {
            Self::Ddr4Fine { .. } => {
                if value.minimum_cycles.is_some() || value.picoseconds > 32_002 {
                    return Err(Error::ValueOutOfRange);
                }
                let coarse = (value.picoseconds / 125).min(255);
                let fine = value.picoseconds - coarse * 125;
                Ok([coarse as u8, fine as u8, 0])
            }
            Self::Word { unit, cycles, .. } => {
                if value.minimum_cycles.is_some() != cycles.is_some()
                    || value.picoseconds % unit != 0
                    || value.picoseconds / unit > u32::from(u16::MAX)
                {
                    return Err(Error::ValueOutOfRange);
                }
                let word = (value.picoseconds / unit) as u16;
                let [low, high] = word.to_le_bytes();
                Ok([low, high, value.minimum_cycles.unwrap_or(0)])
            }
        }
    }

    fn write(self, bytes: &mut [u8], encoded: [u8; 3]) {
        match self {
            Self::Ddr4Fine { coarse, fine } => {
                bytes[coarse] = encoded[0];
                bytes[fine] = encoded[1];
            }
            Self::Word { offset, cycles, .. } => {
                bytes[offset] = encoded[0];
                bytes[offset + 1] = encoded[1];
                if let Some(offset) = cycles {
                    bytes[offset] = encoded[2];
                }
            }
        }
    }
}

impl Image<'_> {
    fn timing_encoding(&self, field: TimingField) -> Result<Encoding, Error> {
        let layout = self.layout()?;
        let encoding = Encoding::for_field(layout, field)?;
        if layout == Layout::Ddr4 {
            let timebase = *self.bytes.get(17).ok_or(Error::Truncated)?;
            if timebase != 0 {
                return Err(Error::UnsupportedTimebase(timebase));
            }
        }
        self.bytes
            .get(..encoding.required_end())
            .ok_or(Error::Truncated)?;
        Ok(encoding)
    }

    /// Read one supported timing as an exact duration and optional clock count.
    ///
    /// Dispatches by memory type, including the selected FPM/EDO, SDRAM, DDR,
    /// DDR2, and DDR3 codecs. Only the required header and field bytes must be
    /// present; a complete image or valid checksum/CRC is not required.
    ///
    /// Legacy integer/fractional nanosecond fields are scaled to picoseconds.
    /// DDR3 accepts the supported 125 ps medium timebase and, in revision 1.1,
    /// rational signed fine offsets; fractional-picosecond results are rejected,
    /// never rounded. DDR4 requires its supported 125 ps medium / 1 ps signed
    /// fine timebases. DDR5 scales stored nanoseconds where required and retains
    /// separately stored nCK constraints, including zero.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for missing required bytes, unsupported
    /// memory/revision/module/field errors for combinations outside the selected
    /// codecs, and [`Error::UnsupportedTimebase`] for unknown DDR4 timebases.
    /// [`Error::InvalidEncoding`] identifies invalid legacy values, invalid DDR3
    /// timebases or fractional results, and negative signed durations.
    pub fn timing(&self, field: TimingField) -> Result<Timing, Error> {
        match self.memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => return legacy::timing(self.bytes, field),
            MemoryType::Ddr3 => return ddr3::timing(self.bytes, field),
            MemoryType::Ddr4 | MemoryType::Ddr5 => {}
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        }
        self.timing_encoding(field)?.decode(self.bytes)
    }
}

impl ImageMut<'_> {
    /// Replace one supported timing without changing unrelated bits or integrity bytes.
    ///
    /// Dispatches to the same generation-specific codecs as [`Image::timing`].
    /// All header, length, and exact-representation checks precede mutation:
    /// every error leaves the entire image unchanged. No rounding, frequency
    /// conversion, inferred nCK, or semantic configuration validation occurs.
    ///
    /// Legacy fields require their exact stored units: whole ns for FPM/EDO
    /// and SDRAM row timings, tenths of ns for SDRAM clock period (1–15.9 ns),
    /// quarter ns for DDR/DDR2 tRP/tRCD, and whole ns for their tRAS.
    /// Undefined zero legacy encodings are rejected.
    /// DDR3 requires a nonzero coarse count fitting the field and an exact
    /// signed fine correction where available; MTB-only values are multiples
    /// of 125 ps. These generations require `minimum_cycles: None`.
    ///
    /// DDR4 also requires no separate count; coarse/fine durations are limited
    /// to 32002 ps and refresh durations to multiples of 125 ps fitting u16.
    /// DDR5 requires `Some(nCK)` exactly for fields storing a count byte (zero
    /// is permitted); nanosecond durations require multiples of 1000 ps, while
    /// other durations must fit u16 picoseconds. Representable zero is not a
    /// configuration-validity guarantee.
    ///
    /// Setting an equal decoded DDR3/DDR4 value preserves alternate coarse/fine
    /// encodings. A valid replacement may overwrite an invalid old duration.
    /// No checksum or CRC is implicitly repaired.
    ///
    /// # Errors
    ///
    /// Header, length, and timebase errors match [`Image::timing`].
    /// [`Error::ValueOutOfRange`] means the new duration/count combination has
    /// no exact supported representation.
    ///
    /// # Example
    ///
    /// This synthetic DDR4 buffer demonstrates preservation, not a real DIMM:
    ///
    /// ```
    /// use libspd::{ImageMut, Timing, TimingField};
    ///
    /// let mut bytes = [0xa5; 256];
    /// bytes[1..4].copy_from_slice(&[0x11, 0x0c, 0x03]);
    /// bytes[17] = 0; // Supported DDR4 timebases.
    /// let before = bytes;
    /// let mut image = ImageMut::new(&mut bytes);
    /// let value = Timing { picoseconds: 833, minimum_cycles: None };
    /// image.set_timing(TimingField::TckMin, value)?;
    /// assert_eq!(image.as_image().timing(TimingField::TckMin)?, value);
    /// for index in 0..bytes.len() {
    ///     if index != 18 && index != 125 {
    ///         assert_eq!(bytes[index], before[index]);
    ///     }
    /// }
    /// // In particular, the stored base and module CRC bytes are unchanged.
    /// assert_eq!(&bytes[126..128], &before[126..128]);
    /// assert_eq!(&bytes[254..256], &before[254..256]);
    /// # Ok::<(), libspd::Error>(())
    /// ```
    pub fn set_timing(&mut self, field: TimingField, value: Timing) -> Result<(), Error> {
        match self.as_image().memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => return legacy::set_timing(self.bytes, field, value),
            MemoryType::Ddr3 => return ddr3::set_timing(self.bytes, field, value),
            MemoryType::Ddr4 | MemoryType::Ddr5 => {}
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        }
        let encoding = self.as_image().timing_encoding(field)?;
        if encoding.decode(self.bytes) == Ok(value) {
            return Ok(());
        }
        let encoded = encoding.encode(value)?;
        encoding.write(self.bytes, encoded);
        Ok(())
    }
}
