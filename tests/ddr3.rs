use libspd::{Error, Image, ImageMut, MemoryType, Timing, TimingField};

// Sparse synthetic values, not manufacturer images or full module fixtures.
fn synthetic(revision: u8) -> [u8; 176] {
    let mut bytes = [0xa5; 176];
    bytes[..4].copy_from_slice(&[0x92, revision, 0x0b, 0x03]);
    bytes[9..12].copy_from_slice(&[0x11, 1, 8]);
    for offset in [12, 16, 17, 18, 20, 22, 23, 24, 27, 29] {
        bytes[offset] = 8;
    }
    bytes[21] = 0x21;
    bytes[25] = 1;
    bytes[28] = 0xc1;
    bytes[34..39].fill(0);
    bytes
}

fn ps(picoseconds: u32) -> Timing {
    Timing {
        picoseconds,
        minimum_cycles: None,
    }
}

// (field, coarse low offset, optional high byte/nibble shift, fine offset)
type FieldEncoding = (TimingField, usize, Option<(usize, u8)>, Option<usize>);
const FIELDS: [FieldEncoding; 10] = [
    (TimingField::TckMin, 12, None, Some(34)),
    (TimingField::Taa, 16, None, Some(35)),
    (TimingField::Twr, 17, None, None),
    (TimingField::Trcd, 18, None, Some(36)),
    (TimingField::Trp, 20, None, Some(37)),
    (TimingField::Tras, 22, Some((21, 0)), None),
    (TimingField::Trc, 23, Some((21, 4)), Some(38)),
    (TimingField::Trfc1, 24, Some((25, 0)), None),
    (TimingField::Trtp, 27, None, None),
    (TimingField::Tfaw, 29, Some((28, 0)), None),
];

fn encoded_end(revision: u8, low: usize, fine: Option<usize>) -> usize {
    if revision != 0x10 {
        if let Some(fine) = fine {
            return fine + 1;
        }
    }
    low + if low == 24 { 2 } else { 1 }
}

fn assert_set_error(bytes: &mut [u8], field: TimingField, value: Timing, error: Error) {
    let original = bytes.to_vec();
    assert_eq!(ImageMut::new(bytes).set_timing(field, value), Err(error));
    assert_eq!(bytes, original);
}

#[test]
fn revision_one_zero_ignores_reserved_fine_bytes() {
    let mut bytes = synthetic(0x10);
    bytes[12] = 15;
    bytes[9] = 0x52; // Samsung 2009: FTB 2.5 ps, but offsets still reserved.
    bytes[34] = 0x80;
    assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Ok(ps(1875)));
    let original = bytes;
    ImageMut::new(&mut bytes)
        .set_timing(TimingField::TckMin, ps(1500))
        .unwrap();
    assert_eq!(bytes[12], 12);
    for offset in 0..bytes.len() {
        if offset != 12 {
            assert_eq!(bytes[offset], original[offset]);
        }
    }
    assert_set_error(
        &mut bytes,
        TimingField::TckMin,
        ps(1501),
        Error::ValueOutOfRange,
    );
}

#[test]
fn signed_offsets_and_nonintegral_fine_timebases_are_exact() {
    for revision in 0x11..0x12 {
        for (field, low, high, fine) in FIELDS {
            let Some(fine) = fine else { continue };
            let mut bytes = synthetic(revision);
            bytes[low] = 9;
            if let Some((offset, shift)) = high {
                bytes[offset] &= !(15 << shift);
            }
            bytes[fine] = 0xca; // Samsung 2011: 9 * 125 - 54 = 1071 ps.
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(1071)));
            bytes[9] = 0x52;
            bytes[fine] = (-2i8) as u8;
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(1120)));
            bytes[fine] = (-1i8) as u8;
            assert_eq!(
                Image::new(&bytes).timing(field),
                Err(Error::InvalidEncoding { offset: fine })
            );
            ImageMut::new(&mut bytes)
                .set_timing(field, ps(1120))
                .unwrap();
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(1120)));
            assert_set_error(&mut bytes, field, ps(1121), Error::ValueOutOfRange);
            bytes[9] = 0x13; // 1/3 ps: arithmetic must combine before dividing.
            bytes[fine] = (-3i8) as u8;
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(1124)));
            ImageMut::new(&mut bytes)
                .set_timing(field, ps(1123))
                .unwrap();
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(1123)));
        }
    }
}

#[test]
fn coarse_packing_is_little_endian_and_preserves_neighbor_nibbles() {
    for (field, low, high, fine) in FIELDS {
        let mut bytes = synthetic(0x11);
        let coarse = if high.is_some() { 0x234u16 } else { 0x34 };
        bytes[low] = coarse as u8;
        if let Some((offset, shift)) = high {
            if field == TimingField::Trfc1 {
                bytes[offset] = (coarse >> 8) as u8;
            } else {
                bytes[offset] = (bytes[offset] & !(15 << shift)) | (2 << shift);
            }
        }
        if let Some(fine) = fine {
            bytes[fine] = 0;
        }
        assert_eq!(
            Image::new(&bytes).timing(field),
            Ok(ps(u32::from(coarse) * 125))
        );
        let original = bytes;
        let replacement = if high.is_some() { 0x345 } else { 0x45 };
        ImageMut::new(&mut bytes)
            .set_timing(field, ps(replacement * 125))
            .unwrap();
        assert_eq!(bytes[low], 0x45);
        assert_eq!(Image::new(&bytes).timing(field), Ok(ps(replacement * 125)));
        if let Some((offset, shift)) = high {
            if field == TimingField::Trfc1 {
                assert_eq!(bytes[offset], 3);
            } else {
                let mask = 15 << shift;
                assert_eq!(bytes[offset] & !mask, original[offset] & !mask);
                assert_eq!((bytes[offset] >> shift) & 15, 3);
            }
        }
        for offset in 0..bytes.len() {
            if offset != low
                && Some(offset) != high.map(|(offset, _)| offset)
                && Some(offset) != fine
            {
                assert_eq!(bytes[offset], original[offset], "{field:?}, byte {offset}");
            }
        }
    }
}

#[test]
fn setters_preserve_alternate_encodings_and_handle_extrema() {
    for (field, low, high, fine) in FIELDS {
        let mut bytes = synthetic(0x11);
        bytes[low] = 1;
        if let Some((offset, shift)) = high {
            if field == TimingField::Trfc1 {
                bytes[offset] = 0;
            } else {
                bytes[offset] &= !(15 << shift);
            }
        }
        if let Some(fine) = fine {
            bytes[fine] = 127;
            let original = bytes;
            ImageMut::new(&mut bytes)
                .set_timing(field, ps(252))
                .unwrap();
            assert_eq!(
                bytes, original,
                "alternate fine encoding changed for {field:?}"
            );
            ImageMut::new(&mut bytes).set_timing(field, ps(0)).unwrap();
            assert_eq!(Image::new(&bytes).timing(field), Ok(ps(0)));
        } else {
            assert_set_error(&mut bytes, field, ps(0), Error::ValueOutOfRange);
            assert_set_error(&mut bytes, field, ps(126), Error::ValueOutOfRange);
        }
        let maximum = match high {
            None => 255,
            Some(_) if field == TimingField::Trfc1 => 65535,
            Some(_) => 4095,
        };
        let last = maximum * 125 + if fine.is_some() { 127 } else { 0 };
        ImageMut::new(&mut bytes)
            .set_timing(field, ps(last))
            .unwrap();
        assert_eq!(Image::new(&bytes).timing(field), Ok(ps(last)));
        assert_set_error(&mut bytes, field, ps(last + 1), Error::ValueOutOfRange);
        assert_set_error(&mut bytes, field, ps(u32::MAX), Error::ValueOutOfRange);
        assert_set_error(
            &mut bytes,
            field,
            Timing {
                picoseconds: 125,
                minimum_cycles: Some(1),
            },
            Error::ValueOutOfRange,
        );
    }
}

#[test]
fn fine_search_finds_representations_beyond_nearest_coarse_and_never_rounds() {
    let mut bytes = synthetic(0x11);
    bytes[9] = 0xf1; // 15 ps FTB, so neighboring MTB counts change residues.
    ImageMut::new(&mut bytes)
        .set_timing(TimingField::TckMin, ps(140))
        .unwrap();
    assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Ok(ps(140)));
    assert_eq!(bytes[12], 1);
    assert_eq!(bytes[34], 1);
    // gcd(125, 15) = 5: 126 cannot be represented by any coarse/fine pair.
    assert_set_error(
        &mut bytes,
        TimingField::TckMin,
        ps(126),
        Error::ValueOutOfRange,
    );
}

#[test]
fn malformed_timebases_values_and_unknown_revisions_are_atomic() {
    for (offset, value, bad_offset) in [
        (9, 0x01, 9),
        (9, 0x10, 9),
        (10, 0, 10),
        (11, 0, 11),
        (11, 16, 10),
    ] {
        let mut bytes = synthetic(0x11);
        bytes[offset] = value;
        let error = Error::InvalidEncoding { offset: bad_offset };
        assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Err(error));
        assert_set_error(&mut bytes, TimingField::TckMin, ps(1500), error);
    }
    let mut bytes = synthetic(0x11);
    bytes[10] = 2;
    bytes[11] = 16; // Equivalent defined MTB, not an assumed literal byte pair.
    assert_eq!(Image::new(&bytes).timing(TimingField::TckMin), Ok(ps(1000)));
    bytes[12] = 1;
    bytes[34] = 0x80;
    assert_eq!(
        Image::new(&bytes).timing(TimingField::TckMin),
        Err(Error::InvalidEncoding { offset: 34 })
    );
    bytes[12] = 0;
    assert_eq!(
        Image::new(&bytes).timing(TimingField::TckMin),
        Err(Error::InvalidEncoding { offset: 12 })
    );
    for revision in [0, 0x09, 0x12, 0x13, 0x14, 0x1f, 0x20, 0xff] {
        let mut bytes = synthetic(revision);
        let original = bytes;
        let error = Error::UnsupportedRevision(revision);
        assert_eq!(Image::new(&bytes).timing(TimingField::Taa), Err(error));
        assert_eq!(Image::new(&bytes).legacy_manufacturing(), Err(error));
        assert_eq!(Image::new(&bytes).base_crc(), Err(error));
        assert_eq!(
            ImageMut::new(&mut bytes).set_part_number(b"SYNTHETIC"),
            Err(error)
        );
        assert_eq!(
            ImageMut::new(&mut bytes).set_serial_number([1, 2, 3, 4]),
            Err(error)
        );
        assert_eq!(ImageMut::new(&mut bytes).repair_base_crc(), Err(error));
        assert_set_error(&mut bytes, TimingField::Taa, ps(1250), error);
        assert_eq!(bytes, original);
    }
}

#[test]
fn every_timing_truncation_is_bounded_and_each_setter_needs_only_its_field() {
    for revision in 0x10..=0x11 {
        for (field, low, _, fine) in FIELDS {
            let full = synthetic(revision);
            let end = encoded_end(revision, low, fine);
            for len in 0..end {
                assert_eq!(
                    Image::new(&full[..len]).timing(field),
                    Err(Error::Truncated),
                    "{field:?} len {len}"
                );
                let mut bytes = full;
                assert_set_error(&mut bytes[..len], field, ps(1500), Error::Truncated);
            }
            let mut bytes = full;
            ImageMut::new(&mut bytes[..end])
                .set_timing(field, ps(1500))
                .unwrap();
            assert_eq!(Image::new(&bytes[..end]).timing(field), Ok(ps(1500)));
        }
    }
}

#[test]
fn raw_metadata_has_ddr3_widths_and_field_local_edits() {
    let mut bytes = synthetic(0x11);
    bytes[117..126].copy_from_slice(&[0x7f, 0x23, 0xe1, 0xfe, 0xdc, 0x91, 0x82, 0x73, 0x64]);
    bytes[128..146].copy_from_slice(b"SYNTHETIC-PART-001");
    bytes[146..148].copy_from_slice(&[0xf1, 0x80]);
    let info = Image::new(&bytes).legacy_manufacturing().unwrap();
    assert_eq!(info.manufacturer, &[0x7f, 0x23]);
    assert_eq!(info.location, 0xe1);
    assert_eq!(info.date_raw, [0xfe, 0xdc]);
    assert_eq!(info.serial, [0x91, 0x82, 0x73, 0x64]);
    assert_eq!(info.part_number, b"SYNTHETIC-PART-001");
    assert_eq!(info.revision, &[0xf1, 0x80]);
    assert_eq!(info.manufacturer.len(), 2);
    assert_eq!(info.part_number.len(), 18);
    let original = bytes;
    ImageMut::new(&mut bytes[..126])
        .set_serial_number([4, 3, 2, 1])
        .unwrap();
    ImageMut::new(&mut bytes[..146])
        .set_part_number(b"NEW")
        .unwrap();
    assert_eq!(&bytes[122..126], &[4, 3, 2, 1]);
    assert_eq!(&bytes[128..146], b"NEW               ");
    for offset in 0..bytes.len() {
        if !(122..126).contains(&offset) && !(128..146).contains(&offset) {
            assert_eq!(bytes[offset], original[offset]);
        }
    }
    for (part, expected) in [
        (&b"1234567890123456789"[..], Error::ValueOutOfRange),
        (&b"bad\0"[..], Error::InvalidEncoding { offset: 131 }),
        (&b"bad\x7f"[..], Error::InvalidEncoding { offset: 131 }),
        (&b"bad\xff"[..], Error::InvalidEncoding { offset: 131 }),
    ] {
        let original = bytes;
        assert_eq!(
            ImageMut::new(&mut bytes).set_part_number(part),
            Err(expected)
        );
        assert_eq!(bytes, original);
    }
    ImageMut::new(&mut bytes).set_part_number(b"").unwrap();
    assert_eq!(&bytes[128..146], &[b' '; 18]);
}

#[test]
fn every_metadata_and_crc_truncation_leaves_output_unchanged() {
    let full = synthetic(0x11);
    for len in 0..148 {
        assert_eq!(
            Image::new(&full[..len]).legacy_manufacturing(),
            Err(Error::Truncated)
        );
        if len < 146 {
            let mut bytes = full;
            assert_eq!(
                ImageMut::new(&mut bytes[..len]).set_part_number(b"X"),
                Err(Error::Truncated)
            );
            assert_eq!(bytes, full);
        }
        if len < 126 {
            let mut bytes = full;
            assert_eq!(
                ImageMut::new(&mut bytes[..len]).set_serial_number([1, 2, 3, 4]),
                Err(Error::Truncated)
            );
            assert_eq!(bytes, full);
        }
        if len < 128 {
            let mut bytes = full;
            assert_eq!(Image::new(&bytes[..len]).base_crc(), Err(Error::Truncated));
            assert_eq!(
                ImageMut::new(&mut bytes[..len]).repair_base_crc(),
                Err(Error::Truncated)
            );
            assert_eq!(bytes, full);
        }
    }
    assert!(Image::new(&full[..148]).legacy_manufacturing().is_ok());
    assert!(Image::new(&full[..128]).base_crc().is_ok());
}

// Independent polynomial long division, rather than calling the core CRC helper.
fn reference_crc(bytes: &[u8]) -> u16 {
    let mut remainder = 0u32;
    for byte in bytes.iter().copied().chain([0, 0]) {
        for bit in (0..8).rev() {
            remainder = (remainder << 1) | u32::from((byte >> bit) & 1);
            if remainder & 0x10000 != 0 {
                remainder ^= 0x11021;
            }
        }
    }
    remainder as u16
}

#[test]
fn both_crc_domains_include_their_last_byte_and_exclude_storage() {
    assert_eq!(reference_crc(b"123456789"), 0x31c3);
    for revision in 0x10..=0x11 {
        for (header, end) in [(0x92, 117), (0x12, 126)] {
            let mut bytes = synthetic(revision);
            bytes[0] = header;
            bytes[126..128].copy_from_slice(&[0x34, 0x12]);
            let expected = reference_crc(&bytes[..end]);
            let check = Image::new(&bytes).base_crc().unwrap();
            assert_eq!(check.stored, 0x1234);
            assert_eq!(check.calculated, expected);
            let original = bytes;
            let repaired = ImageMut::new(&mut bytes[..128]).repair_base_crc().unwrap();
            assert!(repaired.is_valid());
            assert_eq!(repaired.calculated, expected);
            assert_eq!(&bytes[126..128], &expected.to_le_bytes());
            assert_eq!(&bytes[..126], &original[..126]);
            assert_eq!(&bytes[128..], &original[128..]);
            for offset in [116, 117, 125, 126, 127, 128] {
                let mut changed = bytes;
                changed[offset] ^= 1;
                let changed_check = Image::new(&changed).base_crc().unwrap();
                if offset < end {
                    assert_ne!(changed_check.calculated, expected);
                    assert!(!changed_check.is_valid());
                } else {
                    assert_eq!(changed_check.calculated, expected);
                    assert_eq!(changed_check.is_valid(), !(126..128).contains(&offset));
                }
            }
        }
    }
}

#[test]
fn module_byte_is_raw_and_unsupported_domains_and_timings_are_explicit() {
    let mut bytes = synthetic(0x11);
    bytes[3] = 0xf0;
    let image = Image::new(&bytes);
    assert_eq!(image.memory_type(), Ok(MemoryType::Ddr3));
    let identity = image.identity().unwrap();
    assert_eq!(identity.revision, 0x11);
    assert_eq!(identity.module_type, Some(0xf0));
    assert_eq!(image.timing(TimingField::TckMin), Ok(ps(1000)));
    assert!(image.legacy_manufacturing().is_ok());
    assert_eq!(image.checksum(), Err(Error::UnsupportedField));
    assert_eq!(image.module_crc(), Err(Error::UnsupportedField));
    assert_eq!(image.manufacturing(), Err(Error::UnsupportedField));
    for field in [
        TimingField::TckMax,
        TimingField::TrrdS,
        TimingField::TrrdL,
        TimingField::TwtrS,
        TimingField::TwtrL,
        TimingField::Trfc2,
        TimingField::Trfc4,
        TimingField::TrfcSb,
        TimingField::TccdL,
        TimingField::TccdLWr,
        TimingField::TccdLWr2,
    ] {
        assert_eq!(
            Image::new(&bytes).timing(field),
            Err(Error::UnsupportedField)
        );
        assert_set_error(&mut bytes, field, ps(1500), Error::UnsupportedField);
    }
    let original = bytes;
    assert_eq!(
        ImageMut::new(&mut bytes).repair_checksum(),
        Err(Error::UnsupportedField)
    );
    assert_eq!(
        ImageMut::new(&mut bytes).repair_module_crc(),
        Err(Error::UnsupportedField)
    );
    assert_eq!(bytes, original);
}
