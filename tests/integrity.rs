use libspd::{CrcCheck, Error, Image, ImageMut};

// Patterned synthetic image, not a semantically valid module configuration.
fn synthetic() -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..600)
        .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    bytes[..4].copy_from_slice(&[0xff, 0x11, 0x0c, 0x03]);
    bytes
}

#[test]
fn tiny_source_backed_module_crc_vectors() {
    // Observed module-block examples: A4 Advantech SQR-SD4N8G3K2MNGEA
    // rev1.2 section 7 => E2C0; S4 Samsung M471A5244BB0-CRC May 2018
    // SPD table => 08DB. Only four numeric bytes plus zero padding are retained,
    // not a vendor SPD dump. Header bytes below are synthetic layout selectors.
    for (prefix, expected) in [
        ([0x0f, 0x01, 0x02, 0], 0xe2c0u16),
        ([0x0f, 0x11, 0x02, 0], 0x08dbu16),
    ] {
        let mut bytes = [0u8; 256];
        bytes[..4].copy_from_slice(&[0, 0x11, 0x0c, 0x03]);
        bytes[128..132].copy_from_slice(&prefix);
        bytes[254..256].copy_from_slice(&expected.to_le_bytes());
        assert_eq!(
            Image::new(&bytes).module_crc(),
            Ok(CrcCheck {
                stored: expected,
                calculated: expected
            })
        );
        bytes[254] ^= 1;
        let check = Image::new(&bytes).module_crc().unwrap();
        assert_eq!(check.stored, expected ^ 1);
        assert_eq!(check.calculated, expected);
        assert!(!check.is_valid());
        assert_eq!(
            ImageMut::new(&mut bytes).repair_module_crc(),
            Ok(CrcCheck {
                stored: expected,
                calculated: expected
            })
        );
        assert_eq!(&bytes[254..256], &expected.to_le_bytes());
    }
}

#[test]
fn every_truncated_module_block_and_header_is_atomic() {
    let bytes = synthetic();
    for len in 0..256 {
        let mut partial = bytes[..len].to_vec();
        let before = partial.clone();
        assert_eq!(
            Image::new(&partial).module_crc(),
            Err(Error::Truncated),
            "len {len}"
        );
        assert_eq!(
            ImageMut::new(&mut partial).repair_module_crc(),
            Err(Error::Truncated)
        );
        assert_eq!(partial, before);
    }
    assert!(Image::new(&bytes[..256]).module_crc().is_ok());
}

#[test]
fn module_coverage_is_exact_and_independent_of_base_and_manufacturing() {
    let mut bytes = synthetic();
    ImageMut::new(&mut bytes).repair_base_crc().unwrap();
    ImageMut::new(&mut bytes).repair_module_crc().unwrap();
    let image = Image::new(&bytes);
    let module = image.module_crc().unwrap();
    let base = image.base_crc().unwrap();
    assert!(module.is_valid());
    assert!(base.is_valid());
    // Each covered byte affects the checksum; no other byte enters its domain.
    // Header selectors must remain supported for the typed operation.
    for offset in 4..bytes.len() {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        let check = Image::new(&changed).module_crc().unwrap();
        if (128..254).contains(&offset) {
            assert_ne!(
                check.calculated, module.calculated,
                "covered offset {offset}"
            );
            assert_eq!(check.stored, module.stored);
            assert!(!check.is_valid());
            assert_eq!(Image::new(&changed).base_crc().unwrap(), base);
        } else {
            assert_eq!(
                check.calculated, module.calculated,
                "outside offset {offset}"
            );
            if !(254..256).contains(&offset) {
                assert_eq!(check, module);
            } else {
                assert_ne!(check.stored, module.stored);
            }
        }
    }
    bytes[0] ^= 1; // Raw size is deliberately not a typed-layout gate.
    assert_eq!(Image::new(&bytes).module_crc().unwrap(), module);
    bytes[1] = 0x13; // Both supported DDR4 revisions have the same domain.
    assert_eq!(Image::new(&bytes).module_crc().unwrap(), module);
}

#[test]
fn explicit_repair_changes_exactly_two_bytes_and_never_the_base_crc() {
    let mut bytes = synthetic();
    ImageMut::new(&mut bytes).repair_base_crc().unwrap();
    let calculated = Image::new(&bytes).module_crc().unwrap().calculated;
    // Force both stored bytes wrong, so the exact mutation footprint is visible.
    bytes[254..256].copy_from_slice(&(calculated ^ 0xffff).to_le_bytes());
    let before = bytes.clone();
    let base = Image::new(&bytes).base_crc().unwrap();
    assert!(!Image::new(&bytes).module_crc().unwrap().is_valid());
    let repaired = ImageMut::new(&mut bytes).repair_module_crc().unwrap();
    assert_eq!(
        repaired,
        CrcCheck {
            stored: calculated,
            calculated
        }
    );
    assert_eq!(&bytes[..254], &before[..254]);
    assert_eq!(&bytes[256..], &before[256..]);
    assert_ne!(bytes[254], before[254]);
    assert_ne!(bytes[255], before[255]);
    assert_eq!(&bytes[254..256], &calculated.to_le_bytes());
    assert_eq!(Image::new(&bytes).base_crc().unwrap(), base);
    assert_eq!(Image::new(&bytes).module_crc().unwrap(), repaired);
    let before = bytes.clone();
    assert_eq!(
        ImageMut::new(&mut bytes).repair_module_crc().unwrap(),
        repaired
    );
    assert_eq!(bytes, before);
}

#[test]
fn base_crc_mismatch_does_not_gate_module_check_or_repair() {
    let mut bytes = synthetic();
    ImageMut::new(&mut bytes).repair_base_crc().unwrap();
    bytes[126] ^= 1;
    assert!(!Image::new(&bytes).base_crc().unwrap().is_valid());
    assert!(Image::new(&bytes).module_crc().is_ok());
    let before_base = bytes[126..128].to_vec();
    ImageMut::new(&mut bytes).repair_module_crc().unwrap();
    assert_eq!(&bytes[126..128], before_base.as_slice());
    assert!(!Image::new(&bytes).base_crc().unwrap().is_valid());
    let module = Image::new(&bytes).module_crc().unwrap();
    ImageMut::new(&mut bytes).repair_base_crc().unwrap();
    assert_eq!(Image::new(&bytes).module_crc().unwrap(), module);
}

#[test]
fn unsupported_layouts_and_ddr5_are_atomic_even_with_only_a_header() {
    for (header, expected) in [
        ([0, 0x11, 0x77, 0x03], Error::UnsupportedMemoryType(0x77)),
        ([0, 0x12, 0x0c, 0x03], Error::UnsupportedRevision(0x12)),
        ([0, 0x11, 0x0c, 0x83], Error::UnsupportedModuleType(0x83)),
        ([0, 0x10, 0x12, 0x02], Error::UnsupportedField),
        ([0, 0x11, 0x12, 0x02], Error::UnsupportedRevision(0x11)),
        ([0, 0x10, 0x12, 0x82], Error::UnsupportedModuleType(0x82)),
    ] {
        for len in [4, 128, 254, 255, 256, 600] {
            let mut bytes = vec![0xa5; len];
            bytes[..4].copy_from_slice(&header);
            let before = bytes.clone();
            assert_eq!(Image::new(&bytes).module_crc(), Err(expected));
            assert_eq!(ImageMut::new(&mut bytes).repair_module_crc(), Err(expected));
            assert_eq!(bytes, before);
        }
    }
}
