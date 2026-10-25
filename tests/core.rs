use libspd::{crc16_xmodem, CrcCheck, Error, Image, ImageMut, MemoryType};

// Patterned synthetic bytes only; not a JEDEC-valid module description.
fn synthetic(len: usize, memory_type: u8) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..len)
        .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    if len >= 3 {
        bytes[2] = memory_type;
    }
    bytes
}

#[test]
fn xmodem_golden_vector_and_empty_input() {
    assert_eq!(crc16_xmodem(b"123456789"), 0x31c3);
    assert_eq!(crc16_xmodem(b""), 0);
    assert!(CrcCheck {
        stored: 0x31c3,
        calculated: 0x31c3
    }
    .is_valid());
    assert!(!CrcCheck {
        stored: 0,
        calculated: 1
    }
    .is_valid());
}

#[test]
fn headers_accept_partial_images_and_preserve_raw_values() {
    for len in 0..4 {
        let bytes = [0xff; 4];
        assert_eq!(Image::new(&bytes[..len]).header(), Err(Error::Truncated));
        assert_eq!(Image::new(&bytes[..len]).base_crc(), Err(Error::Truncated));
    }
    let bytes = [0xff, 0xa7, 0x89, 0xfe];
    let image = Image::new(&bytes);
    let header = image.header().unwrap();
    assert_eq!(header.size_raw, 0xff);
    assert_eq!(header.revision, 0xa7);
    assert_eq!(header.memory_type, MemoryType::Unknown(0x89));
    assert_eq!(header.module_type_raw, 0xfe);
    assert_eq!(image.as_bytes(), bytes);
    assert_eq!(image.base_crc(), Err(Error::UnsupportedMemoryType(0x89)));
    for raw in 0..=u8::MAX {
        assert_eq!(MemoryType::from_raw(raw).as_raw(), raw);
    }
    assert_eq!(MemoryType::from_raw(0x0c), MemoryType::Ddr4);
    assert_eq!(MemoryType::from_raw(0x12), MemoryType::Ddr5);
}

#[test]
fn truncated_crc_and_unknown_crc_repairs_are_atomic() {
    for (raw, required) in [(0x0c, 128), (0x12, 512)] {
        for len in [4, required - 2, required - 1] {
            let mut bytes = synthetic(len, raw);
            let original = bytes.clone();
            assert!(Image::new(&bytes).header().is_ok());
            assert_eq!(Image::new(&bytes).base_crc(), Err(Error::Truncated));
            assert_eq!(
                ImageMut::new(&mut bytes).repair_base_crc(),
                Err(Error::Truncated)
            );
            assert_eq!(bytes, original);
        }
    }
    for len in [0, 1, 2, 3, 4, 512] {
        let mut bytes = synthetic(len, 0x77);
        let original = bytes.clone();
        let expected = if len < 4 {
            Error::Truncated
        } else {
            Error::UnsupportedMemoryType(0x77)
        };
        assert_eq!(ImageMut::new(&mut bytes).repair_base_crc(), Err(expected));
        assert_eq!(bytes, original);
    }
}

#[test]
fn read_bounds_include_overflow_and_preserve_output_on_error() {
    let bytes = [10, 20, 30, 40];
    let image = Image::new(&bytes);
    let mut output = [0xa5; 2];
    for offset in [3, 5, usize::MAX - 1, usize::MAX] {
        assert_eq!(image.read(offset, &mut output), Err(Error::OutOfBounds));
        assert_eq!(output, [0xa5; 2]);
    }
    image.read(2, &mut output).unwrap();
    assert_eq!(output, [30, 40]);
    image.read(4, &mut []).unwrap();
    assert_eq!(image.read(5, &mut []), Err(Error::OutOfBounds));
    assert_eq!(image.read(usize::MAX, &mut []), Err(Error::OutOfBounds));
    Image::new(&[]).read(0, &mut []).unwrap();
}

#[test]
fn write_bounds_are_checked_before_any_mutation() {
    let mut bytes = [10, 20, 30, 40];
    let original = bytes;
    for offset in [3, 5, usize::MAX - 1, usize::MAX] {
        assert_eq!(
            ImageMut::new(&mut bytes).write(offset, &[1, 2]),
            Err(Error::OutOfBounds)
        );
        assert_eq!(bytes, original);
    }
    let mut image = ImageMut::new(&mut bytes);
    image.write(4, &[]).unwrap();
    assert_eq!(image.write(5, &[]), Err(Error::OutOfBounds));
    assert_eq!(image.write(usize::MAX, &[]), Err(Error::OutOfBounds));
    assert_eq!(image.as_image().as_bytes(), original);
    image.write(2, &[90, 91]).unwrap();
    assert_eq!(image.as_image().as_bytes(), [10, 20, 90, 91]);
    ImageMut::new(&mut []).write(0, &[]).unwrap();
}

#[test]
fn edits_preserve_every_other_byte_and_do_not_repair_crc_implicitly() {
    for (raw, crc_offset) in [(0x0c, 126), (0x12, 510)] {
        let mut bytes = synthetic(1024, raw);
        ImageMut::new(&mut bytes).repair_base_crc().unwrap();
        let before = bytes.clone();
        let replacement = [before[20] ^ 0x80, before[21] ^ 0x40];
        ImageMut::new(&mut bytes).write(20, &replacement).unwrap();
        for (i, (&actual, &original)) in bytes.iter().zip(&before).enumerate() {
            assert_eq!(
                actual,
                if (20..22).contains(&i) {
                    replacement[i - 20]
                } else {
                    original
                }
            );
        }
        assert_eq!(
            &bytes[crc_offset..crc_offset + 2],
            &before[crc_offset..crc_offset + 2]
        );
        assert!(!Image::new(&bytes).base_crc().unwrap().is_valid());
        let edited = bytes.clone();
        let repaired = ImageMut::new(&mut bytes).repair_base_crc().unwrap();
        assert!(repaired.is_valid());
        assert_eq!(Image::new(&bytes).base_crc().unwrap(), repaired);
        assert_eq!(
            &bytes[crc_offset..crc_offset + 2],
            &repaired.calculated.to_le_bytes()
        );
        for i in 0..bytes.len() {
            if !(crc_offset..crc_offset + 2).contains(&i) {
                assert_eq!(bytes[i], edited[i]);
            }
        }
    }
}

#[test]
fn base_crc_uses_format_specific_spans_and_little_endian_storage() {
    for (raw, end) in [(0x0c, 126), (0x12, 510)] {
        let mut bytes = synthetic(1024, raw);
        bytes[end] = 0x34;
        bytes[end + 1] = 0x12;
        let check = Image::new(&bytes).base_crc().unwrap();
        assert_eq!(check.stored, 0x1234);
        assert_eq!(check.calculated, crc16_xmodem(&bytes[..end]));
        assert_eq!(Image::new(&bytes[..end + 2]).base_crc().unwrap(), check);
        bytes[end + 2] ^= 0xff;
        bytes[1023] ^= 0xff;
        assert_eq!(Image::new(&bytes).base_crc().unwrap(), check);
        bytes[end - 1] ^= 1;
        assert_ne!(
            Image::new(&bytes).base_crc().unwrap().calculated,
            check.calculated
        );
    }
    // Byte 200 is outside DDR4 base coverage but inside DDR5 base coverage.
    for raw in [0x0c, 0x12] {
        let mut bytes = synthetic(512, raw);
        let before = Image::new(&bytes).base_crc().unwrap().calculated;
        bytes[200] ^= 1;
        let after = Image::new(&bytes).base_crc().unwrap().calculated;
        assert_eq!(before == after, raw == 0x0c);
    }
}
