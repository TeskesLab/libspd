//! Synthetic in-memory demonstration, not a JEDEC-valid SPD image or a driver.

use libspd::{Error, Image, ImageMut, Read, Timing, TimingField, Write};

// An application-owned adapter; a hardware adapter could implement the same
// traits while keeping addressing, paging, and bus policy outside libspd.
struct SyntheticMemory {
    bytes: [u8; 1024],
}

impl Read for SyntheticMemory {
    type Error = Error;

    fn read(&mut self, offset: usize, output: &mut [u8]) -> Result<(), Self::Error> {
        Image::new(&self.bytes).read(offset, output)
    }
}

impl Write for SyntheticMemory {
    fn write(&mut self, offset: usize, input: &[u8]) -> Result<(), Self::Error> {
        ImageMut::new(&mut self.bytes).write(offset, input)
    }
}

fn main() -> Result<(), Error> {
    println!("Synthetic in-memory example; NOT a JEDEC-valid SPD image.");
    let mut adapter = SyntheticMemory { bytes: [0; 1024] };
    // A supported header selects codecs, but does not make these synthetic bytes
    // a complete or physically valid module description.
    adapter.write(0, &[0x30, 0x10, 0x12, 0x02])?;

    let mut snapshot = [0; 1024];
    adapter.read(0, &mut snapshot)?;
    let image = Image::new(&snapshot);
    println!("Raw header: {:?}", image.header()?);
    println!("Initial base CRC: {:?}", image.base_crc()?);

    let mut edited = ImageMut::new(&mut snapshot);
    edited.set_timing(
        TimingField::TckMin,
        Timing {
            picoseconds: 357,
            minimum_cycles: None,
        },
    )?;
    edited.set_timing(
        TimingField::Trfc1,
        Timing {
            picoseconds: 295_000,
            minimum_cycles: None,
        },
    )?;
    let row_delay = Timing {
        picoseconds: 5000,
        minimum_cycles: Some(8),
    };
    edited.set_timing(TimingField::TrrdL, row_delay)?;
    edited.set_part_number(b"LIBSPD SYNTHETIC")?;
    edited.set_serial_number([1, 2, 3, 4])?;

    let decoded = edited.as_image();
    assert_eq!(decoded.timing(TimingField::TrrdL)?, row_delay);
    assert_eq!(decoded.timing(TimingField::Trfc1)?.picoseconds, 295_000);
    let identity = decoded.manufacturing()?;
    assert_eq!(identity.serial, [1, 2, 3, 4]);
    println!(
        "Part number: {}",
        std::str::from_utf8(identity.part_number).expect("the example sets printable ASCII")
    );
    println!(
        "DDR5 minimum cycle time: {:?}",
        decoded.timing(TimingField::TckMin)?
    );
    println!("DDR5 row delay (duration and nCK): {row_delay:?}");
    let mut before_invalid_edit = [0; 1024];
    before_invalid_edit.copy_from_slice(edited.as_image().as_bytes());
    assert_eq!(
        edited.set_timing(
            TimingField::Trfc1,
            Timing {
                picoseconds: 295_001,
                minimum_cycles: None,
            },
        ),
        Err(Error::ValueOutOfRange)
    );
    assert_eq!(edited.as_image().as_bytes(), &before_invalid_edit);
    let crc_before_repair = edited.as_image().base_crc()?;
    assert_eq!(crc_before_repair.stored, 0);
    println!("CRC after typed edits (not automatically repaired): {crc_before_repair:?}");
    let repaired = edited.repair_base_crc()?;
    assert!(repaired.is_valid());
    println!("Explicitly repaired base CRC: {repaired:?}");

    // Sending the edited snapshot is an explicit application operation, not a
    // side effect of parsing or repair. This adapter touches only local memory.
    adapter.write(0, edited.as_image().as_bytes())?;
    let mut round_trip = [0; 1024];
    adapter.read(0, &mut round_trip)?;
    assert_eq!(round_trip.as_slice(), edited.as_image().as_bytes());
    assert!(Image::new(&round_trip).base_crc()?.is_valid());
    Ok(())
}
