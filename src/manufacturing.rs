//! Raw manufacturing fields for selected legacy, DDR3, DDR4, and DDR5 layouts.

use crate::{ddr3, legacy, Error, Image, ImageMut, Layout, LegacyManufacturing, MemoryType};

/// Raw manufacturing data for the supported DDR4/DDR5 layouts.
///
/// Only `part_number` borrows the original image for lifetime `'a`; numeric
/// fields are copied in on-image byte order. No allocation, text conversion,
/// JEP106 bank/parity/name decoding, BCD date validation, serial endianness
/// interpretation, or revision/stepping interpretation is performed.
/// Use [`LegacyManufacturing`] for earlier generations.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Manufacturing<'a> {
    /// Two raw module-manufacturer identifier bytes, in stored order.
    pub module_manufacturer: [u8; 2],
    /// Raw one-byte manufacturing location code; no location name is inferred.
    pub location: u8,
    /// Two raw date bytes; no year/week or BCD validity is inferred.
    pub date_raw: [u8; 2],
    /// Four raw serial bytes, with no numeric endianness interpretation.
    pub serial: [u8; 4],
    /// Fixed-width borrowed part-number field: 20 bytes for DDR4, 30 for DDR5.
    ///
    /// Includes padding and arbitrary non-ASCII/NUL bytes exactly as stored.
    /// This is neither a UTF-8 string nor a NUL-terminated C string.
    pub part_number: &'a [u8],
    /// Raw one-byte module revision, not a decoded revision number.
    pub module_revision: u8,
    /// Two raw DRAM-manufacturer identifier bytes, in stored order.
    pub dram_manufacturer: [u8; 2],
    /// Raw one-byte DRAM stepping, without vendor-specific interpretation.
    pub dram_stepping: u8,
}

fn manufacturing_layout(layout: Layout) -> (usize, usize) {
    match layout {
        Layout::Ddr4 => (320, 20),
        Layout::Ddr5 => (512, 30),
    }
}

impl<'a> Image<'a> {
    /// Read all supported DDR4/DDR5 manufacturing fields, borrowing the part number.
    ///
    /// Requires 353 bytes for DDR4 or 555 for DDR5 and the supported typed
    /// revision/module header. Does not require a valid CRC or printable text;
    /// unknown field values remain raw. The returned borrow lasts as long as
    /// the original image bytes, not merely the borrow of this `Image`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for missing fields, unsupported header errors
    /// for unknown memory/revision/module codes, and [`Error::UnsupportedField`]
    /// for pre-DDR4 images; use [`Image::legacy_manufacturing`] for those.
    ///
    /// # Example
    ///
    /// A synthetic DDR4 buffer can contain arbitrary raw metadata:
    ///
    /// ```
    /// use libspd::Image;
    ///
    /// let mut bytes = [0u8; 353];
    /// bytes[1..4].copy_from_slice(&[0x11, 0x0c, 0x03]);
    /// bytes[329..349].fill(b' ');
    /// bytes[329..333].copy_from_slice(&[b'P', 0xff, 0, b'N']);
    /// let metadata = Image::new(&bytes).manufacturing()?;
    /// assert_eq!(metadata.part_number.len(), 20);
    /// assert_eq!(&metadata.part_number[..4], &[b'P', 0xff, 0, b'N']);
    /// assert_eq!(metadata.part_number.as_ptr(), bytes[329..].as_ptr());
    /// # Ok::<(), libspd::Error>(())
    /// ```
    pub fn manufacturing(&self) -> Result<Manufacturing<'a>, Error> {
        match self.memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2
            | MemoryType::Ddr3 => return Err(Error::UnsupportedField),
            MemoryType::Ddr4 | MemoryType::Ddr5 => {}
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        }
        let (start, width) = manufacturing_layout(self.layout()?);
        let section = self
            .bytes
            .get(start..start + width + 13)
            .ok_or(Error::Truncated)?;
        let part_end = 9 + width;
        Ok(Manufacturing {
            module_manufacturer: [section[0], section[1]],
            location: section[2],
            date_raw: [section[3], section[4]],
            serial: [section[5], section[6], section[7], section[8]],
            part_number: &section[9..part_end],
            module_revision: section[part_end],
            dram_manufacturer: [section[part_end + 1], section[part_end + 2]],
            dram_stepping: section[part_end + 3],
        })
    }

    /// Read the complete raw manufacturing section for supported pre-DDR4 images.
    ///
    /// Requires 99 bytes for FPM/EDO, SDRAM, DDR, or DDR2, or 148 for DDR3.
    /// The generation-specific revision must be supported, but no checksum or
    /// CRC validity is required. The identifier, part number, and revision are
    /// borrowed for the original image lifetime `'a`; all other fields are
    /// copied. No text/date/manufacturer conventions are inferred.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] for missing bytes, unsupported memory or
    /// revision errors outside the selected codecs, and
    /// [`Error::UnsupportedField`] for DDR4/DDR5 (use [`Image::manufacturing`]).
    pub fn legacy_manufacturing(&self) -> Result<LegacyManufacturing<'a>, Error> {
        match self.memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => legacy::manufacturing(self.bytes),
            MemoryType::Ddr3 => ddr3::manufacturing(self.bytes),
            MemoryType::Ddr4 | MemoryType::Ddr5 => Err(Error::UnsupportedField),
            MemoryType::Unknown(raw) => Err(Error::UnsupportedMemoryType(raw)),
        }
    }
}

impl ImageMut<'_> {
    /// Set a printable ASCII part number and right-pad its field with spaces.
    ///
    /// Accepts bytes `0x20..=0x7e` only: no NUL terminator, UTF-8 conversion, or
    /// truncation. The maximum is 18 bytes before DDR4, 20 for DDR4, or 30 for
    /// DDR5. Empty input clears the whole field to spaces. Only the supported
    /// header and complete part-number field must be present, not the aggregate
    /// manufacturing section. All unrelated bytes and checksum/CRC storage
    /// remain unchanged on success; every error leaves the entire image unchanged.
    ///
    /// # Errors
    ///
    /// Returns unsupported header errors or [`Error::Truncated`] as appropriate,
    /// [`Error::ValueOutOfRange`] for overlength input, and
    /// [`Error::InvalidEncoding`] for nonprintable input. The latter offset is
    /// the destination image offset of the first rejected byte.
    pub fn set_part_number(&mut self, part_number: &[u8]) -> Result<(), Error> {
        match self.as_image().memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => return legacy::set_part_number(self.bytes, part_number),
            MemoryType::Ddr3 => return ddr3::set_part_number(self.bytes, part_number),
            MemoryType::Ddr4 | MemoryType::Ddr5 => {}
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        }
        let (start, width) = manufacturing_layout(self.as_image().layout()?);
        let start = start + 9;
        let output = self
            .bytes
            .get_mut(start..start + width)
            .ok_or(Error::Truncated)?;
        if part_number.len() > width {
            return Err(Error::ValueOutOfRange);
        }
        for (index, &byte) in part_number.iter().enumerate() {
            if !(0x20..=0x7e).contains(&byte) {
                return Err(Error::InvalidEncoding {
                    offset: start + index,
                });
            }
        }
        output[..part_number.len()].copy_from_slice(part_number);
        output[part_number.len()..].fill(b' ');
        Ok(())
    }

    /// Replace exactly the four raw serial-number bytes, in supplied order.
    ///
    /// Performs no normalization, numeric endian conversion, or text decoding.
    /// Requires only the supported header and serial field, not all manufacturing
    /// fields. Unrelated bytes and checksum/CRC storage are unchanged on success;
    /// every error leaves the entire image unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Truncated`] if required bytes are absent, or unsupported
    /// memory/revision/module errors when the typed layout is not supported.
    pub fn set_serial_number(&mut self, serial: [u8; 4]) -> Result<(), Error> {
        match self.as_image().memory_type()? {
            MemoryType::Fpm
            | MemoryType::Edo
            | MemoryType::Sdram
            | MemoryType::Ddr
            | MemoryType::Ddr2 => return legacy::set_serial_number(self.bytes, serial),
            MemoryType::Ddr3 => return ddr3::set_serial_number(self.bytes, serial),
            MemoryType::Ddr4 | MemoryType::Ddr5 => {}
            MemoryType::Unknown(raw) => return Err(Error::UnsupportedMemoryType(raw)),
        }
        let (start, _) = manufacturing_layout(self.as_image().layout()?);
        let output = self
            .bytes
            .get_mut(start + 5..start + 9)
            .ok_or(Error::Truncated)?;
        output.copy_from_slice(&serial);
        Ok(())
    }
}
