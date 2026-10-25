use libspd::{Error, Image, MemoryType};

// Synthetic bytes deliberately distinguish the old capacity/row slots from
// the actual revision field. No complete SPD module is represented here.
#[test]
fn legacy_identity_does_not_misread_capacity_as_revision_or_rows_as_module_type() {
    for (raw, kind) in [
        (1, MemoryType::Fpm),
        (2, MemoryType::Edo),
        (4, MemoryType::Sdram),
        (7, MemoryType::Ddr),
        (8, MemoryType::Ddr2),
    ] {
        let mut bytes = [0; 63];
        bytes[1] = 8;
        bytes[2] = raw;
        bytes[3] = 13;
        bytes[20] = 2;
        bytes[62] = 0x10;
        let identity = Image::new(&bytes).identity().unwrap();
        assert_eq!(identity.memory_type, kind);
        assert_eq!(identity.revision, 0x10);
        assert_eq!(identity.module_type, if raw == 8 { Some(2) } else { None });
        let header = Image::new(&bytes).header().unwrap();
        assert_eq!(header.revision, 8); // Raw slot, not semantic identity.
        assert_eq!(header.module_type_raw, 13);
        for len in 0..63 {
            assert_eq!(Image::new(&bytes[..len]).identity(), Err(Error::Truncated));
        }
        assert_eq!(Image::new(&bytes[..3]).memory_type(), Ok(kind));
    }
}

#[test]
fn modern_identity_reports_raw_revision_and_preserves_present_zero_module_byte() {
    for (raw, kind) in [
        (0x0b, MemoryType::Ddr3),
        (0x0c, MemoryType::Ddr4),
        (0x12, MemoryType::Ddr5),
    ] {
        let bytes = [0, 0xff, raw, 0];
        let identity = Image::new(&bytes).identity().unwrap();
        assert_eq!(identity.memory_type, kind);
        assert_eq!(identity.revision, 0xff); // Identity is not layout validation.
        assert_eq!(identity.module_type, Some(0));
        for len in 0..4 {
            assert_eq!(Image::new(&bytes[..len]).identity(), Err(Error::Truncated));
        }
    }
    assert_eq!(
        Image::new(&[0, 0, 0xff]).memory_type(),
        Ok(MemoryType::Unknown(0xff))
    );
    assert_eq!(
        Image::new(&[0, 0, 0xff]).identity(),
        Err(Error::UnsupportedMemoryType(0xff))
    );
    for len in 0..3 {
        assert_eq!(
            Image::new(&[0; 3][..len]).memory_type(),
            Err(Error::Truncated)
        );
    }
}
