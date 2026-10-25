//! Independent DDR4 module-block CRC checking and explicit storage-byte repair.
//!
//! These operations do not validate the base CRC, manufacturing metadata, or
//! the semantic configuration. Ordinary reads and field edits never repair
//! integrity storage implicitly.

use crate::{crc16_xmodem, CrcCheck, Error, Image, ImageMut, Layout, MemoryType};

impl Image<'_> {
    /// Check the DDR4 module-specific CRC independently of the base CRC.
    ///
    /// Uses CRC-16/XMODEM over bytes `128..254`, comparing little-endian storage
    /// at `254..256`. Requires a supported DDR4 typed revision/module layout
    /// and at least 256 bytes. Neither a valid base CRC nor a complete image is
    /// required. A mismatch is successful `Ok(CrcCheck)` data, not a decoding
    /// error; use [`CrcCheck::is_valid`] to test equality.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedField`] for recognized pre-DDR4 generations,
    /// or [`Error::UnsupportedMemoryType`] for unknown memory codes. DDR4/DDR5
    /// first undergo typed revision/module validation; a supported DDR5 layout
    /// then returns [`Error::UnsupportedField`]. Missing required bytes return
    /// [`Error::Truncated`].
    pub fn module_crc(&self) -> Result<CrcCheck, Error> {
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
        match self.layout()? {
            Layout::Ddr4 => {}
            Layout::Ddr5 => return Err(Error::UnsupportedField),
        }
        let block = self.bytes.get(128..256).ok_or(Error::Truncated)?;
        Ok(CrcCheck {
            stored: u16::from_le_bytes([block[126], block[127]]),
            calculated: crc16_xmodem(&block[..126]),
        })
    }
}

impl ImageMut<'_> {
    /// Explicitly repair only the DDR4 module CRC's two storage bytes.
    ///
    /// Recomputes CRC-16/XMODEM over `128..254` and writes the little-endian
    /// result only to `254..256`. All validation and calculation precede
    /// mutation. Success returns the post-repair check (`stored == calculated`);
    /// it does not certify any other integrity domain or module configuration.
    /// Neither base CRC storage at `126..128` nor any covered data or
    /// manufacturing byte is changed.
    ///
    /// # Errors
    ///
    /// Has the same layout and length errors as [`Image::module_crc`].
    /// Every error leaves the complete image unchanged.
    pub fn repair_module_crc(&mut self) -> Result<CrcCheck, Error> {
        let check = self.as_image().module_crc()?;
        self.bytes[254..256].copy_from_slice(&check.calculated.to_le_bytes());
        Ok(CrcCheck {
            stored: check.calculated,
            calculated: check.calculated,
        })
    }
}
