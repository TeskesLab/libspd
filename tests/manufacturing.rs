use libspd::{Error, Image, ImageMut, Manufacturing};

// Patterned synthetic images, not complete or semantically valid SPD dumps.
fn synthetic(ddr5: bool) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..600).map(|i| (i as u8).wrapping_mul(37)).collect();
    bytes[..4].copy_from_slice(if ddr5 {
        &[0xff, 0x10, 0x12, 0x02]
    } else {
        &[0xff, 0x11, 0x0c, 0x03]
    });
    bytes
}

// The returned part number must borrow the input, not the temporary Image.
fn decode(bytes: &[u8]) -> Manufacturing<'_> {
    Image::new(bytes).manufacturing().unwrap()
}

#[test]
fn source_verified_offsets_return_raw_fields_and_borrow_original_part_number() {
    // Offset provenance: Advantech SQR-SD4N8G3K2MNGEA rev1.2 section 7,
    // and SQR-UD5N16G5K6HNAG rev1.0 section 10. Values here are synthetic.
    for (ddr5, start, width) in [(false, 320, 20), (true, 512, 30)] {
        let mut bytes = synthetic(ddr5);
        bytes[start..start + 9]
            .copy_from_slice(&[0xff, 0x80, 0xe1, 0xfa, 0xff, 0, 0xff, 0x80, 0x7f]);
        bytes[start + 9..start + 9 + width].fill(0x80);
        let end = start + 9 + width;
        bytes[end..end + 4].copy_from_slice(&[0x91, 0xff, 0x81, 0xfe]);
        let before = bytes.clone();
        let fields = decode(&bytes);
        assert_eq!(fields.module_manufacturer, [0xff, 0x80]);
        assert_eq!(fields.location, 0xe1);
        assert_eq!(fields.date_raw, [0xfa, 0xff]); // Deliberately not BCD.
        assert_eq!(fields.serial, [0, 0xff, 0x80, 0x7f]);
        assert_eq!(fields.part_number, &bytes[start + 9..end]);
        assert_eq!(fields.part_number.as_ptr(), bytes[start + 9..end].as_ptr());
        assert_eq!(fields.module_revision, 0x91);
        assert_eq!(fields.dram_manufacturer, [0xff, 0x81]);
        assert_eq!(fields.dram_stepping, 0xfe);
        assert_eq!(bytes, before);
    }
}

#[test]
fn every_truncation_is_rejected_and_setters_require_only_their_own_field() {
    for (ddr5, start, width) in [(false, 320, 20), (true, 512, 30)] {
        let bytes = synthetic(ddr5);
        let serial_end = start + 9;
        let part_end = serial_end + width;
        let required = part_end + 4;
        // Includes every header, section, individual field, and field-end boundary.
        for len in 0..required {
            let mut partial = bytes[..len].to_vec();
            assert_eq!(
                Image::new(&partial).manufacturing(),
                Err(Error::Truncated),
                "decode len {len}"
            );
            let before = partial.clone();
            let result = ImageMut::new(&mut partial).set_serial_number([1, 2, 3, 4]);
            if len < serial_end {
                assert_eq!(result, Err(Error::Truncated));
                assert_eq!(partial, before);
            } else {
                assert_eq!(result, Ok(()));
                let mut expected = before;
                expected[start + 5..serial_end].copy_from_slice(&[1, 2, 3, 4]);
                assert_eq!(partial, expected);
            }
            let mut partial = bytes[..len].to_vec();
            let before = partial.clone();
            let result = ImageMut::new(&mut partial).set_part_number(b"X");
            if len < part_end {
                assert_eq!(result, Err(Error::Truncated));
                assert_eq!(partial, before);
            } else {
                assert_eq!(result, Ok(()));
                let mut expected = before;
                expected[serial_end..part_end].fill(b' ');
                expected[serial_end] = b'X';
                assert_eq!(partial, expected);
            }
        }
        assert!(Image::new(&bytes[..required]).manufacturing().is_ok());
    }
}

#[test]
fn part_number_ascii_bounds_padding_empty_and_exact_width_preserve_all_other_bytes() {
    for (ddr5, start, width) in [(false, 329, 20), (true, 521, 30)] {
        let mut bytes = synthetic(ddr5);
        for length in 0..=width {
            let part: Vec<u8> = (0..length)
                .map(|i| if i % 2 == 0 { 0x20 } else { 0x7e })
                .collect();
            let mut expected = bytes.clone();
            expected[start..start + width].fill(b' ');
            expected[start..start + length].copy_from_slice(&part);
            ImageMut::new(&mut bytes).set_part_number(&part).unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(decode(&bytes).part_number, &expected[start..start + width]);
            // Writing the same representation does not touch any other field.
            ImageMut::new(&mut bytes).set_part_number(&part).unwrap();
            assert_eq!(bytes, expected);
        }
        for length in [width + 1, width + 17] {
            let before = bytes.clone();
            assert_eq!(
                ImageMut::new(&mut bytes).set_part_number(&vec![b'A'; length]),
                Err(Error::ValueOutOfRange)
            );
            assert_eq!(bytes, before);
        }
        // Check the entire printable ASCII alphabet, not just its boundaries.
        for byte in 0x20..=0x7e {
            ImageMut::new(&mut bytes).set_part_number(&[byte]).unwrap();
            assert_eq!(bytes[start], byte);
            assert!(bytes[start + 1..start + width].iter().all(|&b| b == b' '));
        }
    }
}

#[test]
fn every_nonprintable_byte_is_rejected_atomically_at_each_input_position() {
    for (ddr5, start, width) in [(false, 329, 20), (true, 521, 30)] {
        let mut bytes = synthetic(ddr5);
        let before = bytes.clone();
        for index in 0..width {
            for byte in 0..=u8::MAX {
                if (0x20..=0x7e).contains(&byte) {
                    continue;
                }
                let mut part = vec![b'A'; width];
                part[index] = byte;
                assert_eq!(
                    ImageMut::new(&mut bytes).set_part_number(&part),
                    Err(Error::InvalidEncoding {
                        offset: start + index
                    })
                );
                assert_eq!(bytes, before);
            }
        }
    }
}

#[test]
fn raw_serial_edits_preserve_all_other_bytes_and_both_crc_storages() {
    for (ddr5, start) in [(false, 325), (true, 517)] {
        let mut bytes = synthetic(ddr5);
        for serial in [[0, 0xff, 0x80, 0x7f], [0xff; 4], [0; 4]] {
            let mut expected = bytes.clone();
            expected[start..start + 4].copy_from_slice(&serial);
            ImageMut::new(&mut bytes).set_serial_number(serial).unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(decode(&bytes).serial, serial);
        }
    }
}

#[test]
fn unsupported_layouts_are_rejected_before_decoding_or_mutation() {
    for (header, error) in [
        ([0, 0x11, 0x77, 0x03], Error::UnsupportedMemoryType(0x77)),
        ([0, 0x12, 0x0c, 0x03], Error::UnsupportedRevision(0x12)),
        ([0, 0x11, 0x0c, 0x83], Error::UnsupportedModuleType(0x83)),
        ([0, 0x11, 0x12, 0x02], Error::UnsupportedRevision(0x11)),
        ([0, 0x10, 0x12, 0x82], Error::UnsupportedModuleType(0x82)),
    ] {
        for len in [4, 600] {
            let mut bytes = vec![0xa5; len];
            bytes[..4].copy_from_slice(&header);
            let before = bytes.clone();
            assert_eq!(Image::new(&bytes).manufacturing(), Err(error));
            assert_eq!(ImageMut::new(&mut bytes).set_part_number(b"A"), Err(error));
            assert_eq!(
                ImageMut::new(&mut bytes).set_serial_number([0; 4]),
                Err(error)
            );
            assert_eq!(bytes, before);
        }
    }
    let mut bytes = synthetic(false);
    bytes[1] = 0x13;
    assert!(Image::new(&bytes).manufacturing().is_ok());
    ImageMut::new(&mut bytes).set_part_number(b"REV13").unwrap();
    ImageMut::new(&mut bytes).set_serial_number([0; 4]).unwrap();
}

#[test]
fn invalid_crcs_do_not_gate_manufacturing_or_setters() {
    for ddr5 in [false, true] {
        let mut bytes = synthetic(ddr5);
        ImageMut::new(&mut bytes).repair_base_crc().unwrap();
        let base_offset = if ddr5 { 510 } else { 126 };
        bytes[base_offset] ^= 1;
        if !ddr5 {
            ImageMut::new(&mut bytes).repair_module_crc().unwrap();
            bytes[254] ^= 1;
            assert!(!Image::new(&bytes).module_crc().unwrap().is_valid());
        }
        assert!(!Image::new(&bytes).base_crc().unwrap().is_valid());
        let base = bytes[base_offset..base_offset + 2].to_vec();
        let module = bytes[254..256].to_vec();
        assert!(Image::new(&bytes).manufacturing().is_ok());
        ImageMut::new(&mut bytes)
            .set_part_number(b"CRC INDEPENDENT")
            .unwrap();
        ImageMut::new(&mut bytes)
            .set_serial_number([1, 0xff, 0, 0x80])
            .unwrap();
        assert_eq!(&bytes[base_offset..base_offset + 2], base.as_slice());
        assert_eq!(&bytes[254..256], module.as_slice());
    }
}
