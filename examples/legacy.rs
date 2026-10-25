// SPDX-License-Identifier: Apache-2.0
//! Synthetic selected fields only, not complete module descriptions or real dumps.
use libspd::{Error, Image, ImageMut, MemoryType, Timing, TimingField};

fn main() -> Result<(), Error> {
    let generations = [
        (MemoryType::Fpm, 1, TimingField::Trac, 60_000, 65_000),
        (MemoryType::Edo, 1, TimingField::Trac, 60_000, 65_000),
        (MemoryType::Sdram, 1, TimingField::Trp, 20_000, 21_000),
        (MemoryType::Ddr, 0x10, TimingField::Trp, 18_000, 18_250),
        (MemoryType::Ddr2, 0x12, TimingField::Trp, 12_500, 12_750),
        (MemoryType::Ddr3, 0x11, TimingField::TckMin, 1_071, 1_072),
    ];
    for (memory_type, revision, field, initial, edited) in generations {
        let is_ddr3 = memory_type == MemoryType::Ddr3;
        for short_coverage in [false, true] {
            if short_coverage && !is_ddr3 {
                continue;
            }
            let mut bytes = [0u8; 256];
            bytes[0] = if short_coverage { 0x80 } else { 0 };
            bytes[1] = if is_ddr3 { revision } else { 0xa5 };
            bytes[2] = memory_type.as_raw();
            bytes[3] = 3;
            bytes[20] = 4;
            bytes[62] = revision;
            match memory_type {
                MemoryType::Fpm | MemoryType::Edo => {
                    bytes[9] = 60;
                    bytes[10] = 15;
                }
                MemoryType::Sdram => bytes[27] = 20,
                MemoryType::Ddr => bytes[27] = 0x48,
                MemoryType::Ddr2 => bytes[27] = 0x32,
                MemoryType::Ddr3 => {
                    bytes[9] = 0x11;
                    bytes[10] = 1;
                    bytes[11] = 8;
                    bytes[12] = 9;
                    bytes[34] = 0xca; // 9*125ps + (-54ps) = 1071ps.
                }
                MemoryType::Ddr4 | MemoryType::Ddr5 | MemoryType::Unknown(_) => {
                    unreachable!("the synthetic matrix contains only legacy formats")
                }
            }
            let raw = Image::new(&bytes).header()?;
            let identity = Image::new(&bytes).identity()?;
            assert_eq!(identity.memory_type, memory_type);
            assert_eq!(identity.revision, revision);
            assert_eq!(raw.revision, if is_ddr3 { revision } else { 0xa5 });
            assert_eq!(
                identity.module_type,
                match memory_type {
                    MemoryType::Ddr2 => Some(4),
                    MemoryType::Ddr3 => Some(3),
                    _ => None,
                }
            );
            assert_eq!(Image::new(&bytes).timing(field)?.picoseconds, initial);
            if is_ddr3 {
                assert!(ImageMut::new(&mut bytes).repair_base_crc()?.is_valid());
                assert_eq!(Image::new(&bytes).checksum(), Err(Error::UnsupportedField));
            } else {
                let independent_sum = bytes[..63]
                    .iter()
                    .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
                let check = ImageMut::new(&mut bytes).repair_checksum()?;
                assert!(check.is_valid());
                assert_eq!(check.stored, independent_sum);
                assert_eq!(Image::new(&bytes).base_crc(), Err(Error::UnsupportedField));
            }
            let value = Timing {
                picoseconds: edited,
                minimum_cycles: None,
            };
            ImageMut::new(&mut bytes).set_timing(field, value)?;
            assert_eq!(Image::new(&bytes).timing(field)?, value);
            if is_ddr3 {
                assert!(!Image::new(&bytes).base_crc()?.is_valid());
                ImageMut::new(&mut bytes).repair_base_crc()?;
            } else {
                assert!(!Image::new(&bytes).checksum()?.is_valid());
                ImageMut::new(&mut bytes).repair_checksum()?;
            }
            let before = bytes;
            assert_eq!(
                ImageMut::new(&mut bytes).set_timing(
                    field,
                    Timing {
                        minimum_cycles: Some(1),
                        ..value
                    },
                ),
                Err(Error::ValueOutOfRange)
            );
            assert_eq!(bytes, before);
            ImageMut::new(&mut bytes).set_part_number(b"SYNTHETIC-LEGACY")?;
            ImageMut::new(&mut bytes).set_serial_number([1, 0x80, 0xff, 0x37])?;
            let manufacturing = Image::new(&bytes).legacy_manufacturing()?;
            assert_eq!(
                manufacturing.manufacturer.len(),
                if is_ddr3 { 2 } else { 8 }
            );
            assert_eq!(manufacturing.part_number, b"SYNTHETIC-LEGACY  ");
            assert_eq!(manufacturing.serial, [1, 0x80, 0xff, 0x37]);
            assert_eq!(manufacturing.revision.len(), 2);
            if is_ddr3 {
                ImageMut::new(&mut bytes).repair_base_crc()?;
                bytes[120] ^= 1; // Covered only when byte0 bit7 is clear.
                assert_eq!(Image::new(&bytes).base_crc()?.is_valid(), short_coverage);
                assert!(ImageMut::new(&mut bytes).repair_base_crc()?.is_valid());
            } else {
                assert!(Image::new(&bytes).checksum()?.is_valid());
            }
            println!("{memory_type:?}: timing {initial}->{edited}ps, raw metadata, explicit integrity repair");
        }
    }
    Ok(())
}
