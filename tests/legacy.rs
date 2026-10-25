use libspd::{Error, Image, ImageMut, MemoryType, Timing, TimingField};

// Sparse synthetic vectors, not full module images or vendor SPD dumps.
const FORMATS: [(u8, u8); 5] = [(1, 1), (2, 1), (4, 1), (7, 0x10), (8, 0x12)];

fn synthetic(memory_type: u8, revision: u8, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    if let Some(byte) = bytes.get_mut(2) {
        *byte = memory_type;
    }
    if let Some(byte) = bytes.get_mut(62) {
        *byte = revision;
    }
    bytes
}

fn duration(picoseconds: u32) -> Timing {
    Timing {
        picoseconds,
        minimum_cycles: None,
    }
}

#[test]
fn access_times_are_not_misidentified_as_clock_or_cas_latency() {
    for memory_type in [1, 2] {
        let mut bytes = synthetic(memory_type, 1, 99);
        bytes[9] = 60;
        bytes[10] = 15;
        assert_eq!(
            Image::new(&bytes).timing(TimingField::Trac),
            Ok(duration(60_000))
        );
        assert_eq!(
            Image::new(&bytes).timing(TimingField::Tcac),
            Ok(duration(15_000))
        );
        for field in [TimingField::TckMin, TimingField::Taa, TimingField::Trp] {
            assert_eq!(
                Image::new(&bytes).timing(field),
                Err(Error::UnsupportedField)
            );
            let before = bytes.clone();
            assert_eq!(
                ImageMut::new(&mut bytes).set_timing(field, duration(10_000)),
                Err(Error::UnsupportedField)
            );
            assert_eq!(bytes, before);
        }
        let before = bytes.clone();
        ImageMut::new(&mut bytes)
            .set_timing(TimingField::Trac, duration(65_000))
            .unwrap();
        assert_eq!(bytes[9], 65);
        assert_eq!(&bytes[..9], &before[..9]);
        assert_eq!(&bytes[10..], &before[10..]);
        ImageMut::new(&mut bytes)
            .set_timing(TimingField::Tcac, duration(255_000))
            .unwrap();
        assert_eq!(bytes[10], 255);
    }
}

#[test]
fn sdram_cycle_time_uses_decimal_nibbles_not_binary_or_ddr_fractions() {
    let mut bytes = synthetic(4, 1, 64);
    for (raw, ps) in [(0x10, 1_000), (0x75, 7_500), (0xa5, 10_500), (0xf9, 15_900)] {
        bytes[9] = raw;
        assert_eq!(
            Image::new(&bytes).timing(TimingField::TckMin),
            Ok(duration(ps))
        );
        let before = bytes.clone();
        ImageMut::new(&mut bytes)
            .set_timing(TimingField::TckMin, duration(ps))
            .unwrap();
        assert_eq!(bytes, before);
    }
    for raw in [0x00, 0x05, 0x7a, 0x7d, 0xff] {
        bytes[9] = raw;
        assert_eq!(
            Image::new(&bytes).timing(TimingField::TckMin),
            Err(Error::InvalidEncoding { offset: 9 })
        );
    }
    let before = bytes.clone();
    for ps in [0, 900, 1_001, 7_750, 15_901, 16_000, u32::MAX] {
        assert_eq!(
            ImageMut::new(&mut bytes).set_timing(TimingField::TckMin, duration(ps)),
            Err(Error::ValueOutOfRange)
        );
        assert_eq!(bytes, before);
    }
    ImageMut::new(&mut bytes)
        .set_timing(TimingField::TckMin, duration(12_300))
        .unwrap();
    assert_eq!(bytes[9], 0xc3);
    assert_eq!(
        Image::new(&bytes).timing(TimingField::Taa),
        Err(Error::UnsupportedField)
    );
}

#[test]
fn selected_row_timings_use_generation_specific_units() {
    // TI B.2.27/29/30: integer ns. Micron DDR/DDR2 maps: quarter ns for
    // tRP/tRCD, integer ns for tRAS. Numeric expectations are independent.
    for (memory_type, revision, row_raw, expected, edited, encoded) in [
        (4, 1, 20, 20_000, 21_000, 21),
        (7, 0x10, 0x48, 18_000, 18_250, 0x49),
        (8, 0x12, 0x32, 12_500, 12_750, 0x33),
    ] {
        for (field, offset) in [(TimingField::Trp, 27), (TimingField::Trcd, 29)] {
            let mut bytes = synthetic(memory_type, revision, 64);
            bytes[offset] = row_raw;
            bytes[63] = 0xa7;
            assert_eq!(Image::new(&bytes).timing(field), Ok(duration(expected)));
            let before = bytes.clone();
            ImageMut::new(&mut bytes)
                .set_timing(field, duration(edited))
                .unwrap();
            assert_eq!(bytes[offset], encoded);
            assert_eq!(&bytes[..offset], &before[..offset]);
            assert_eq!(&bytes[offset + 1..], &before[offset + 1..]);
        }
        let mut bytes = synthetic(memory_type, revision, 63);
        bytes[30] = 45;
        assert_eq!(
            Image::new(&bytes).timing(TimingField::Tras),
            Ok(duration(45_000))
        );
        ImageMut::new(&mut bytes)
            .set_timing(TimingField::Tras, duration(46_000))
            .unwrap();
        assert_eq!(bytes[30], 46);
        if memory_type != 4 {
            assert_eq!(
                Image::new(&bytes).timing(TimingField::TckMin),
                Err(Error::UnsupportedField)
            );
        }
    }
}

#[test]
fn undefined_early_timings_and_inexact_edits_are_rejected_atomically() {
    for (memory_type, revision, field, offset, unit, max) in [
        (1, 1, TimingField::Trac, 9, 1_000, 255_000),
        (2, 1, TimingField::Tcac, 10, 1_000, 255_000),
        (4, 1, TimingField::Trp, 27, 1_000, 255_000),
        (4, 1, TimingField::Trcd, 29, 1_000, 255_000),
        (4, 1, TimingField::Tras, 30, 1_000, 255_000),
        (7, 0x10, TimingField::Trp, 27, 250, 63_750),
        (8, 0x12, TimingField::Trcd, 29, 250, 63_750),
        (8, 0x12, TimingField::Tras, 30, 1_000, 255_000),
    ] {
        let mut bytes = synthetic(memory_type, revision, 99);
        if memory_type <= 4 {
            assert_eq!(
                Image::new(&bytes).timing(field),
                Err(Error::InvalidEncoding { offset })
            );
            let before = bytes.clone();
            assert_eq!(
                ImageMut::new(&mut bytes).set_timing(field, duration(0)),
                Err(Error::ValueOutOfRange)
            );
            assert_eq!(bytes, before);
        }
        bytes[offset] = 10;
        let before = bytes.clone();
        for value in [
            duration(unit + 1),
            duration(max + unit),
            duration(u32::MAX),
            Timing {
                picoseconds: unit,
                minimum_cycles: Some(1),
            },
        ] {
            assert_eq!(
                ImageMut::new(&mut bytes).set_timing(field, value),
                Err(Error::ValueOutOfRange)
            );
            assert_eq!(bytes, before);
        }
        ImageMut::new(&mut bytes)
            .set_timing(field, duration(max))
            .unwrap();
        assert_eq!(bytes[offset], 255);
    }
}

#[test]
fn raw_manufacturing_preserves_every_field_and_vendor_conventions() {
    for (memory_type, revision) in FORMATS {
        let mut bytes = synthetic(memory_type, revision, 110);
        bytes[64..72].copy_from_slice(&[0x7f, 0xff, 0x80, 0x2c, 0, 3, 4, 5]);
        bytes[72] = 0xfe;
        bytes[73..91].copy_from_slice(b"RAW-PART-123456789");
        bytes[80] = 0xff;
        bytes[91..93].copy_from_slice(&[0xc2, 0xf9]);
        bytes[93..95].copy_from_slice(&[0xff, 0xa5]);
        bytes[95..99].copy_from_slice(&[0x81, 0, 0xff, 3]);
        let info = Image::new(&bytes).legacy_manufacturing().unwrap();
        assert_eq!(info.manufacturer, &bytes[64..72]);
        assert_eq!(info.location, 0xfe);
        assert_eq!(info.part_number, &bytes[73..91]);
        assert_eq!(info.revision, &[0xc2, 0xf9]);
        assert_eq!(info.date_raw, [0xff, 0xa5]);
        assert_eq!(info.serial, [0x81, 0, 0xff, 3]);
        assert_eq!(
            Image::new(&bytes).manufacturing(),
            Err(Error::UnsupportedField)
        );
    }
}

#[test]
fn part_and_serial_edits_touch_only_their_own_fields() {
    for (memory_type, revision) in FORMATS {
        let mut bytes = synthetic(memory_type, revision, 110);
        bytes[63..].fill(0xa5);
        let before = bytes.clone();
        ImageMut::new(&mut bytes).set_part_number(b"TEST").unwrap();
        assert_eq!(&bytes[73..77], b"TEST");
        assert_eq!(&bytes[77..91], &[b' '; 14]);
        assert_eq!(&bytes[..73], &before[..73]);
        assert_eq!(&bytes[91..], &before[91..]);
        for part in [
            &b"1234567890123456789"[..],
            &b"x\x1f"[..],
            &b"x\x7f"[..],
            &b"x\xff"[..],
        ] {
            let original = bytes.clone();
            let expected = if part.len() > 18 {
                Error::ValueOutOfRange
            } else {
                Error::InvalidEncoding { offset: 74 }
            };
            assert_eq!(
                ImageMut::new(&mut bytes).set_part_number(part),
                Err(expected)
            );
            assert_eq!(bytes, original);
        }
        ImageMut::new(&mut bytes).set_part_number(b"").unwrap();
        assert_eq!(&bytes[73..91], &[b' '; 18]);
        let before = bytes.clone();
        ImageMut::new(&mut bytes)
            .set_serial_number([0xff, 0, 0x81, 4])
            .unwrap();
        assert_eq!(&bytes[95..99], &[0xff, 0, 0x81, 4]);
        assert_eq!(&bytes[..95], &before[..95]);
        assert_eq!(&bytes[99..], &before[99..]);
    }
}

#[test]
fn additive_checksum_is_low_sum_not_twos_complement_or_crc() {
    for ((memory_type, revision), expected) in FORMATS.into_iter().zip([196, 197, 199, 217, 220]) {
        let mut bytes = synthetic(memory_type, revision, 100);
        bytes[0] = 250;
        bytes[1] = 200;
        bytes[63] = 0x55;
        bytes[64..].fill(0xff);
        let check = Image::new(&bytes).checksum().unwrap();
        assert_eq!(check.stored, 0x55);
        assert_eq!(check.calculated, expected);
        assert!(!check.is_valid());
        let before = bytes.clone();
        let repaired = ImageMut::new(&mut bytes).repair_checksum().unwrap();
        assert!(repaired.is_valid());
        assert_eq!(repaired.stored, expected);
        assert_eq!(&bytes[..63], &before[..63]);
        assert_eq!(&bytes[64..], &before[64..]);
        bytes[99] ^= 0xff;
        assert!(Image::new(&bytes).checksum().unwrap().is_valid());
        bytes[61] = 1;
        assert_eq!(
            Image::new(&bytes).checksum().unwrap().calculated,
            expected.wrapping_add(1)
        );
        assert!(!Image::new(&bytes).checksum().unwrap().is_valid());
        assert_eq!(Image::new(&bytes).base_crc(), Err(Error::UnsupportedField));
        assert_eq!(
            Image::new(&bytes).module_crc(),
            Err(Error::UnsupportedField)
        );
    }
}

#[test]
fn every_truncation_is_bounded_and_failed_edits_are_atomic() {
    for (memory_type, revision) in FORMATS {
        let full = synthetic(memory_type, revision, 100);
        let field = if memory_type <= 2 {
            TimingField::Trac
        } else {
            TimingField::Trp
        };
        for len in 0..99 {
            let mut bytes = full[..len].to_vec();
            let before = bytes.clone();
            if len < 63 {
                assert_eq!(Image::new(&bytes).timing(field), Err(Error::Truncated));
                assert_eq!(
                    ImageMut::new(&mut bytes).set_timing(field, duration(20_000)),
                    Err(Error::Truncated)
                );
                assert_eq!(bytes, before);
            }
            if len < 64 {
                assert_eq!(Image::new(&bytes).checksum(), Err(Error::Truncated));
                assert_eq!(
                    ImageMut::new(&mut bytes).repair_checksum(),
                    Err(Error::Truncated)
                );
                assert_eq!(bytes, before);
            }
            assert_eq!(
                Image::new(&bytes).legacy_manufacturing(),
                Err(Error::Truncated)
            );
            if len < 91 {
                assert_eq!(
                    ImageMut::new(&mut bytes).set_part_number(b"X"),
                    Err(Error::Truncated)
                );
                assert_eq!(bytes, before);
            }
            assert_eq!(
                ImageMut::new(&mut bytes).set_serial_number([1, 2, 3, 4]),
                Err(Error::Truncated)
            );
            assert_eq!(bytes, before);
        }
        let mut part_only = full[..91].to_vec();
        ImageMut::new(&mut part_only)
            .set_part_number(b"123456789012345678")
            .unwrap();
        assert_eq!(&part_only[73..91], b"123456789012345678");
        assert!(Image::new(&full[..99]).legacy_manufacturing().is_ok());
    }
}

#[test]
fn revisions_come_from_byte62_and_unknown_layouts_never_mutate() {
    for (memory_type, revision) in FORMATS {
        let mut bytes = synthetic(memory_type, revision, 99);
        bytes[1] = 0xee; // EEPROM capacity, not an SPD revision.
        bytes[3] = 0xff; // Row-address byte, not a module type.
        bytes[20] = 0xa4; // Raw DDR2 module type; common fields do not depend on it.
        let identity = Image::new(&bytes).identity().unwrap();
        assert_eq!(identity.memory_type, MemoryType::from_raw(memory_type));
        assert_eq!(identity.revision, revision);
        assert_eq!(
            identity.module_type,
            if memory_type == 8 { Some(0xa4) } else { None }
        );
        assert!(Image::new(&bytes).legacy_manufacturing().is_ok());
        bytes[62] = 0xff;
        let before = bytes.clone();
        assert_eq!(
            Image::new(&bytes).legacy_manufacturing(),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(
            Image::new(&bytes).checksum(),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(
            ImageMut::new(&mut bytes).repair_checksum(),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(
            ImageMut::new(&mut bytes).set_part_number(b"X"),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(
            ImageMut::new(&mut bytes).set_serial_number([1, 2, 3, 4]),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(
            ImageMut::new(&mut bytes).set_timing(TimingField::Trp, duration(20_000)),
            Err(Error::UnsupportedRevision(0xff))
        );
        assert_eq!(bytes, before);
    }
    for memory_type in [1, 2, 4] {
        let bytes = synthetic(memory_type, 0, 99);
        assert!(Image::new(&bytes).legacy_manufacturing().is_ok());
    }
    for (memory_type, revision) in [(1, 2), (2, 2), (4, 2), (7, 0x11), (8, 0x11)] {
        let bytes = synthetic(memory_type, revision, 99);
        assert_eq!(
            Image::new(&bytes).legacy_manufacturing(),
            Err(Error::UnsupportedRevision(revision))
        );
    }
}
