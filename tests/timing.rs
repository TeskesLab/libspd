use libspd::{Error, Image, ImageMut, Timing, TimingField};

// Sparse synthetic images only; these are not complete or JEDEC-valid modules.
fn synthetic(len: usize, ddr5: bool) -> Vec<u8> {
    let mut bytes = vec![0; len];
    let header = if ddr5 {
        [0, 0x10, 0x12, 0x02]
    } else {
        [0, 0x13, 0x0c, 0x03]
    };
    for (byte, value) in bytes.iter_mut().zip(header) {
        *byte = value;
    }
    bytes
}

fn timing(picoseconds: u32, minimum_cycles: Option<u8>) -> Timing {
    Timing {
        picoseconds,
        minimum_cycles,
    }
}

const DDR4_FINE: [(TimingField, usize, usize); 8] = [
    (TimingField::TckMin, 18, 125),
    (TimingField::TckMax, 19, 124),
    (TimingField::Taa, 24, 123),
    (TimingField::Trcd, 25, 122),
    (TimingField::Trp, 26, 121),
    (TimingField::TrrdS, 38, 119),
    (TimingField::TrrdL, 39, 118),
    (TimingField::TccdL, 40, 117),
];

const DDR4_WORDS: [(TimingField, usize); 3] = [
    (TimingField::Trfc1, 30),
    (TimingField::Trfc2, 32),
    (TimingField::Trfc4, 34),
];

const DDR5_WORDS: [(TimingField, usize, u32, Option<usize>); 19] = [
    (TimingField::TckMin, 20, 1, None),
    (TimingField::TckMax, 22, 1, None),
    (TimingField::Taa, 30, 1, None),
    (TimingField::Trcd, 32, 1, None),
    (TimingField::Trp, 34, 1, None),
    (TimingField::Tras, 36, 1, None),
    (TimingField::Trc, 38, 1, None),
    (TimingField::Twr, 40, 1, None),
    (TimingField::Trfc1, 42, 1_000, None),
    (TimingField::Trfc2, 44, 1_000, None),
    (TimingField::TrfcSb, 46, 1_000, None),
    (TimingField::TrrdL, 70, 1, Some(72)),
    (TimingField::TccdL, 73, 1, Some(75)),
    (TimingField::TccdLWr, 76, 1, Some(78)),
    (TimingField::TccdLWr2, 79, 1, Some(81)),
    (TimingField::Tfaw, 82, 1, Some(84)),
    (TimingField::TwtrL, 85, 1, Some(87)),
    (TimingField::TwtrS, 88, 1, Some(90)),
    (TimingField::Trtp, 91, 1, Some(93)),
];

fn assert_atomic_error(bytes: &mut [u8], field: TimingField, value: Timing, error: Error) {
    let original = bytes.to_vec();
    assert_eq!(
        ImageMut::new(bytes).set_timing(field, value),
        Err(error),
        "field {field:?}, value {value:?}"
    );
    assert_eq!(bytes, original, "failed setter changed {field:?}");
}

#[test]
fn sparse_known_values_use_signed_fine_offsets_and_explicit_ddr5_units() {
    // Independent arithmetic examples from the source notes, not fixture tables:
    // Samsung M471A5244BB0-CRC p.1: 7*125-42 = 833 ps.
    let mut ddr4 = synthetic(126, false);
    ddr4[18] = 7;
    ddr4[125] = 0xd6;
    assert_eq!(
        Image::new(&ddr4).timing(TimingField::TckMin),
        Ok(timing(833, None))
    );
    // Advantech SQR-UD5N16G5K6HNAG §10 pp.13-14 numeric examples.
    let mut ddr5 = synthetic(94, true);
    ddr5[20..22].copy_from_slice(&[0x65, 0x01]);
    ddr5[22..24].copy_from_slice(&[0xf2, 0x03]);
    ddr5[30..32].copy_from_slice(&[0x80, 0x3e]);
    ddr5[42..44].copy_from_slice(&[0x27, 0x01]);
    ddr5[70..73].copy_from_slice(&[0x88, 0x13, 8]);
    for (field, expected) in [
        (TimingField::TckMin, timing(357, None)),
        (TimingField::TckMax, timing(1_010, None)),
        (TimingField::Taa, timing(16_000, None)),
        (TimingField::Trfc1, timing(295_000, None)),
        (TimingField::TrrdL, timing(5_000, Some(8))),
    ] {
        assert_eq!(Image::new(&ddr5).timing(field), Ok(expected));
    }
}

#[test]
fn every_ddr4_fine_field_reads_signed_arithmetic_without_wrapping() {
    for (field, coarse, fine) in DDR4_FINE {
        for (medium, signed, expected) in [
            (7, -42i8, 833),
            (1, -125, 0),
            (0, 127, 127),
            (255, -128, 31_747),
            (255, 127, 32_002),
        ] {
            let mut bytes = synthetic(fine + 1, false);
            bytes[coarse] = medium;
            bytes[fine] = signed as u8;
            assert_eq!(Image::new(&bytes).timing(field), Ok(timing(expected, None)));
        }
        for (medium, signed) in [(0, -128i8), (0, -1), (1, -128)] {
            let mut bytes = synthetic(fine + 1, false);
            bytes[coarse] = medium;
            bytes[fine] = signed as u8;
            assert_eq!(
                Image::new(&bytes).timing(field),
                Err(Error::InvalidEncoding { offset: fine })
            );
        }
    }
}

#[test]
fn every_ddr4_word_is_little_endian_and_scaled_by_125_ps() {
    for (field, offset) in DDR4_WORDS {
        let mut bytes = synthetic(offset + 2, false);
        bytes[offset..offset + 2].copy_from_slice(&[0x34, 0x12]);
        assert_eq!(Image::new(&bytes).timing(field), Ok(timing(582_500, None)));
    }
}

#[test]
fn every_ddr5_word_uses_its_exact_unit_and_retains_cycles() {
    for (field, offset, unit, cycles) in DDR5_WORDS {
        let end = cycles.map_or(offset + 2, |offset| offset + 1);
        let mut bytes = synthetic(end, true);
        bytes[offset..offset + 2].copy_from_slice(&[0x34, 0x12]);
        if let Some(offset) = cycles {
            bytes[offset] = 173;
        }
        assert_eq!(
            Image::new(&bytes).timing(field),
            Ok(timing(4_660 * unit, cycles.map(|_| 173)))
        );
    }
}

#[test]
fn fine_setters_write_only_exact_canonical_coarse_and_fine_bytes() {
    for (field, coarse, fine) in DDR4_FINE {
        for (picoseconds, medium, adjustment) in [
            (0, 0, 0),
            (1, 0, 1),
            (124, 0, 124),
            (125, 1, 0),
            (126, 1, 1),
            (31_875, 255, 0),
            (32_002, 255, 127),
        ] {
            let mut bytes = synthetic(512, false);
            bytes[126..128].copy_from_slice(&[0x7a, 0xa7]);
            bytes[254..256].copy_from_slice(&[0xb2, 0x2b]);
            // Existing valid but different value forces the setter path even at zero.
            bytes[coarse] = 2;
            bytes[fine] = 0;
            let mut expected = bytes.clone();
            expected[coarse] = medium;
            expected[fine] = adjustment;
            let value = timing(picoseconds, None);
            ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(Image::new(&bytes).timing(field), Ok(value));
        }
    }
}

#[test]
fn unchanged_fine_values_preserve_every_alternate_representation() {
    for (field, coarse, fine) in DDR4_FINE {
        for (medium, adjustment, picoseconds) in [
            (7, -42i8, 833),
            (1, -125, 0),
            (0, 127, 127),
            (255, 127, 32_002),
        ] {
            let mut bytes = synthetic(512, false);
            bytes[coarse] = medium;
            bytes[fine] = adjustment as u8;
            bytes[126..128].copy_from_slice(&[0xc3, 0x3c]);
            let original = bytes.clone();
            ImageMut::new(&mut bytes)
                .set_timing(field, timing(picoseconds, None))
                .unwrap();
            assert_eq!(bytes, original, "normalized {field:?}");
        }
    }
}

#[test]
fn a_valid_setter_can_replace_an_invalid_negative_old_duration() {
    for (field, coarse, fine) in DDR4_FINE {
        let mut bytes = synthetic(fine + 1, false);
        bytes[coarse] = 0;
        bytes[fine] = 0x80;
        let mut expected = bytes.clone();
        expected[coarse] = 6;
        expected[fine] = 83;
        ImageMut::new(&mut bytes)
            .set_timing(field, timing(833, None))
            .unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(Image::new(&bytes).timing(field), Ok(timing(833, None)));
    }
}

#[test]
fn word_setters_change_only_their_storage_and_preserve_crc_bytes() {
    for (field, offset) in DDR4_WORDS {
        let mut bytes = synthetic(512, false);
        bytes[126..128].copy_from_slice(&[0x12, 0x34]);
        bytes[254..256].copy_from_slice(&[0x56, 0x78]);
        let mut expected = bytes.clone();
        expected[offset..offset + 2].copy_from_slice(&[0x34, 0x12]);
        let value = timing(582_500, None);
        ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(Image::new(&bytes).timing(field), Ok(value));
        let original = bytes.clone();
        ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
        assert_eq!(bytes, original);
    }
    for (field, offset, unit, cycles) in DDR5_WORDS {
        let mut bytes = synthetic(1_024, true);
        bytes[510..512].copy_from_slice(&[0x9a, 0xbc]);
        let mut expected = bytes.clone();
        expected[offset..offset + 2].copy_from_slice(&[0x34, 0x12]);
        if let Some(offset) = cycles {
            expected[offset] = 173;
        }
        let value = timing(4_660 * unit, cycles.map(|_| 173));
        ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(Image::new(&bytes).timing(field), Ok(value));
        let original = bytes.clone();
        ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
        assert_eq!(bytes, original);
    }
}

fn check_truncations(ddr5: bool, field: TimingField, end: usize, value: Timing) {
    for len in 0..end {
        let mut bytes = synthetic(len, ddr5);
        assert_eq!(
            Image::new(&bytes).timing(field),
            Err(Error::Truncated),
            "field {field:?}, len {len}"
        );
        assert_atomic_error(&mut bytes, field, value, Error::Truncated);
    }
    // Exactly the needed bytes suffice; no full-image or CRC requirement.
    let mut bytes = synthetic(end, ddr5);
    ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
    assert_eq!(Image::new(&bytes).timing(field), Ok(value));
}

#[test]
fn every_supported_field_checks_all_truncations_before_atomic_mutation() {
    for (field, _, fine) in DDR4_FINE {
        check_truncations(false, field, fine + 1, timing(833, None));
    }
    for (field, offset) in DDR4_WORDS {
        check_truncations(false, field, offset + 2, timing(125, None));
    }
    for (field, offset, unit, cycles) in DDR5_WORDS {
        let end = cycles.map_or(offset + 2, |offset| offset + 1);
        check_truncations(true, field, end, timing(unit, cycles.map(|_| 8)));
    }
}

#[test]
fn ddr4_fine_ranges_and_cycle_presence_are_checked_before_mutation() {
    for (field, coarse, fine) in DDR4_FINE {
        let mut bytes = synthetic(126, false);
        bytes[coarse] = 7;
        bytes[fine] = 0xd6;
        for value in [
            timing(32_003, None),
            timing(u32::MAX, None),
            timing(833, Some(0)),
            timing(833, Some(255)),
        ] {
            assert_atomic_error(&mut bytes, field, value, Error::ValueOutOfRange);
        }
    }
}

#[test]
fn ddr4_word_ranges_require_exact_125_ps_divisibility() {
    for (field, offset) in DDR4_WORDS {
        let mut bytes = synthetic(offset + 2, false);
        for picoseconds in [0, 125, 8_191_875] {
            let value = timing(picoseconds, None);
            ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
            assert_eq!(Image::new(&bytes).timing(field), Ok(value));
        }
        assert_eq!(&bytes[offset..offset + 2], &[0xff, 0xff]);
        for value in [
            timing(1, None),
            timing(124, None),
            timing(8_191_876, None),
            timing(8_192_000, None),
            timing(u32::MAX, None),
            timing(8_191_875, Some(0)),
            timing(8_191_875, Some(255)),
        ] {
            assert_atomic_error(&mut bytes, field, value, Error::ValueOutOfRange);
        }
    }
}

#[test]
fn ddr5_ranges_and_unit_precision_are_exact_for_every_field() {
    for (field, offset, unit, cycles) in DDR5_WORDS {
        let mut bytes = synthetic(94, true);
        let max = 65_535 * unit;
        for picoseconds in [0, unit, max] {
            let value = timing(picoseconds, cycles.map(|_| 255));
            ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
            assert_eq!(Image::new(&bytes).timing(field), Ok(value));
        }
        assert_eq!(&bytes[offset..offset + 2], &[0xff, 0xff]);
        for picoseconds in [max + unit, u32::MAX] {
            assert_atomic_error(
                &mut bytes,
                field,
                timing(picoseconds, cycles.map(|_| 255)),
                Error::ValueOutOfRange,
            );
        }
        if unit == 1_000 {
            for picoseconds in [1, 999, 1_001, max - 1] {
                assert_atomic_error(
                    &mut bytes,
                    field,
                    timing(picoseconds, None),
                    Error::ValueOutOfRange,
                );
            }
        }
    }
}

#[test]
fn ddr5_cycle_constraints_require_presence_not_nonzero_values() {
    for (field, _, _, cycles) in DDR5_WORDS {
        let mut bytes = synthetic(94, true);
        if let Some(offset) = cycles {
            for minimum in [0, 255] {
                let value = timing(1, Some(minimum));
                ImageMut::new(&mut bytes).set_timing(field, value).unwrap();
                assert_eq!(bytes[offset], minimum);
                assert_eq!(Image::new(&bytes).timing(field), Ok(value));
            }
            // Equal duration alone does not permit a missing nCK constraint.
            assert_atomic_error(&mut bytes, field, timing(1, None), Error::ValueOutOfRange);
            // Changing only the clock count is a real edit.
            let mut expected = bytes.clone();
            expected[offset] = 7;
            ImageMut::new(&mut bytes)
                .set_timing(field, timing(1, Some(7)))
                .unwrap();
            assert_eq!(bytes, expected);
        } else {
            for minimum in [0, 255] {
                assert_atomic_error(
                    &mut bytes,
                    field,
                    timing(0, Some(minimum)),
                    Error::ValueOutOfRange,
                );
            }
        }
    }
}

#[test]
fn unsupported_fields_are_not_guessed_or_mutated() {
    for ddr5 in [false, true] {
        let mut bytes = synthetic(512, ddr5);
        for raw in 1..=21 {
            let field = TimingField::from_raw(raw).unwrap();
            let supported = if ddr5 {
                DDR5_WORDS.iter().any(|entry| entry.0 == field)
            } else {
                DDR4_FINE.iter().any(|entry| entry.0 == field)
                    || DDR4_WORDS.iter().any(|entry| entry.0 == field)
            };
            if !supported {
                assert_eq!(
                    Image::new(&bytes).timing(field),
                    Err(Error::UnsupportedField)
                );
                assert_atomic_error(&mut bytes, field, timing(0, None), Error::UnsupportedField);
            }
        }
    }
}

#[test]
fn every_ddr4_timing_rejects_unknown_timebases_without_normalization() {
    for field in DDR4_FINE
        .iter()
        .map(|entry| entry.0)
        .chain(DDR4_WORDS.iter().map(|entry| entry.0))
    {
        for timebase in [1, 0x40, 0x80, 0xff] {
            let mut bytes = synthetic(126, false);
            bytes[17] = timebase;
            assert_eq!(
                Image::new(&bytes).timing(field),
                Err(Error::UnsupportedTimebase(timebase))
            );
            assert_atomic_error(
                &mut bytes,
                field,
                timing(0, None),
                Error::UnsupportedTimebase(timebase),
            );
        }
    }
    // Byte 17 is not a DDR5 timebase discriminator.
    let mut bytes = synthetic(24, true);
    bytes[17] = 0xff;
    ImageMut::new(&mut bytes)
        .set_timing(TimingField::TckMin, timing(357, None))
        .unwrap();
    assert_eq!(
        Image::new(&bytes).timing(TimingField::TckMin),
        Ok(timing(357, None))
    );
}

#[test]
fn layout_errors_are_isolated_from_raw_header_and_byte_access() {
    for (ddr5, header, error) in [
        (
            false,
            [0, 0x13, 0x99, 0x03],
            Error::UnsupportedMemoryType(0x99),
        ),
        (
            false,
            [0, 0x12, 0x0c, 0x03],
            Error::UnsupportedRevision(0x12),
        ),
        (
            false,
            [0, 0x13, 0x0c, 0x83],
            Error::UnsupportedModuleType(0x83),
        ),
        (
            false,
            [0, 0x13, 0x0c, 0x02],
            Error::UnsupportedModuleType(0x02),
        ),
        (
            true,
            [0, 0x11, 0x12, 0x02],
            Error::UnsupportedRevision(0x11),
        ),
        (
            true,
            [0, 0x10, 0x12, 0x82],
            Error::UnsupportedModuleType(0x82),
        ),
        (
            true,
            [0, 0x10, 0x12, 0x03],
            Error::UnsupportedModuleType(0x03),
        ),
        // Validation order is generation, revision, then module type.
        (
            true,
            [0, 0xff, 0x99, 0xff],
            Error::UnsupportedMemoryType(0x99),
        ),
        (
            true,
            [0, 0xff, 0x12, 0xff],
            Error::UnsupportedRevision(0xff),
        ),
    ] {
        let mut bytes = synthetic(512, ddr5);
        bytes[..4].copy_from_slice(&header);
        assert!(Image::new(&bytes).header().is_ok());
        assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Err(error));
        assert_atomic_error(&mut bytes, TimingField::TckMin, timing(125, None), error);
        ImageMut::new(&mut bytes).write(18, &[7]).unwrap();
        let mut raw = [0];
        Image::new(&bytes).read(18, &mut raw).unwrap();
        assert_eq!(raw, [7]);
    }
}

#[test]
fn supported_revisions_do_not_invent_a_byte_zero_or_crc_gate() {
    for (ddr5, revision) in [(false, 0x11), (false, 0x13), (true, 0x10)] {
        for size_raw in [0, 0xff] {
            let mut bytes = synthetic(126, ddr5);
            bytes[0] = size_raw;
            bytes[1] = revision;
            let value = timing(125, None);
            ImageMut::new(&mut bytes)
                .set_timing(TimingField::TckMin, value)
                .unwrap();
            assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Ok(value));
            assert_eq!(bytes[0], size_raw);
            assert_eq!(bytes[1], revision);
        }
    }
}
