// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::mem::{align_of, offset_of, size_of};
use std::ptr::{null, null_mut};

#[test]
fn public_layouts_match_the_c_header() {
    assert_eq!(size_of::<SpdStatus>(), 4);
    assert_eq!(size_of::<SpdHeader>(), 4);
    assert_eq!(align_of::<SpdHeader>(), 1);
    assert_eq!(offset_of!(SpdHeader, size_raw), 0);
    assert_eq!(offset_of!(SpdHeader, revision), 1);
    assert_eq!(offset_of!(SpdHeader, memory_type), 2);
    assert_eq!(offset_of!(SpdHeader, module_type_raw), 3);
    assert_eq!(size_of::<SpdCrcCheck>(), 4);
    assert_eq!(align_of::<SpdCrcCheck>(), align_of::<u16>());
    assert_eq!(offset_of!(SpdCrcCheck, stored), 0);
    assert_eq!(offset_of!(SpdCrcCheck, calculated), 2);
    assert_eq!(size_of::<SpdTiming>(), 12);
    assert_eq!(align_of::<SpdTiming>(), align_of::<u32>());
    assert_eq!(offset_of!(SpdTiming, picoseconds), 0);
    assert_eq!(offset_of!(SpdTiming, minimum_cycles), 4);
    assert_eq!(offset_of!(SpdTiming, has_minimum_cycles), 8);
    assert_eq!(size_of::<SpdManufacturing>(), 44);
    assert_eq!(align_of::<SpdManufacturing>(), 1);
    assert_eq!(offset_of!(SpdManufacturing, module_manufacturer), 0);
    assert_eq!(offset_of!(SpdManufacturing, location), 2);
    assert_eq!(offset_of!(SpdManufacturing, date_raw), 3);
    assert_eq!(offset_of!(SpdManufacturing, serial), 5);
    assert_eq!(offset_of!(SpdManufacturing, part_number), 9);
    assert_eq!(offset_of!(SpdManufacturing, part_number_len), 39);
    assert_eq!(offset_of!(SpdManufacturing, module_revision), 40);
    assert_eq!(offset_of!(SpdManufacturing, dram_manufacturer), 41);
    assert_eq!(offset_of!(SpdManufacturing, dram_stepping), 43);
    assert_eq!(size_of::<SpdIdentityInfo>(), 4);
    assert_eq!(align_of::<SpdIdentityInfo>(), 1);
    assert_eq!(offset_of!(SpdIdentityInfo, has_module_type), 3);
    assert_eq!(size_of::<SpdChecksumCheck>(), 2);
    assert_eq!(offset_of!(SpdChecksumCheck, calculated), 1);
    assert_eq!(size_of::<SpdLegacyManufacturingInfo>(), 36);
    assert_eq!(align_of::<SpdLegacyManufacturingInfo>(), 1);
    assert_eq!(offset_of!(SpdLegacyManufacturingInfo, manufacturer_len), 8);
    assert_eq!(offset_of!(SpdLegacyManufacturingInfo, part_number), 16);
    assert_eq!(offset_of!(SpdLegacyManufacturingInfo, revision), 34);
    let pointer_size = size_of::<*mut c_void>();
    assert_eq!(size_of::<Option<SpdReadCallback>>(), pointer_size);
    assert_eq!(size_of::<Option<SpdWriteCallback>>(), pointer_size);
    assert_eq!(offset_of!(SpdIo, context), 0);
    assert_eq!(offset_of!(SpdIo, read), pointer_size);
    assert_eq!(offset_of!(SpdIo, write), 2 * pointer_size);
    assert_eq!(size_of::<SpdIo>(), 3 * pointer_size);
    assert_eq!(spd_abi_version(), 1);
}

#[test]
fn empty_null_buffers_are_valid_but_required_outputs_are_not_optional() {
    let mut header = SpdHeader {
        size_raw: 9,
        revision: 8,
        memory_type: 7,
        module_type_raw: 6,
    };
    let before = header;
    let mut check = SpdCrcCheck {
        stored: 1,
        calculated: 2,
    };
    let check_before = check;
    let mut crc = 0xffff;
    // SAFETY: Empty null buffers are allowed; nonnull outputs are live and
    // disjoint. Invalid required-null outputs are rejected before dereference.
    unsafe {
        assert_eq!(spd_header(null(), 0, &mut header), SPD_TRUNCATED);
        assert_eq!(header, before);
        assert_eq!(spd_base_crc(null(), 0, &mut check), SPD_TRUNCATED);
        assert_eq!(
            spd_repair_base_crc(null_mut(), 0, &mut check),
            SPD_TRUNCATED
        );
        assert_eq!(check, check_before);
        assert_eq!(spd_crc16_xmodem(null(), 0, &mut crc), SPD_OK);
        assert_eq!(crc, 0);
        assert_eq!(spd_image_read(null(), 0, 0, null_mut(), 0), SPD_OK);
        assert_eq!(spd_image_write(null_mut(), 0, 0, null(), 0), SPD_OK);
        assert_eq!(
            spd_image_read(null(), 0, 1, null_mut(), 0),
            SPD_OUT_OF_BOUNDS
        );
        assert_eq!(
            spd_image_write(null_mut(), 0, 1, null(), 0),
            SPD_OUT_OF_BOUNDS
        );
        assert_eq!(spd_header(null(), 0, null_mut()), SPD_INVALID_ARGUMENT);
        assert_eq!(spd_base_crc(null(), 0, null_mut()), SPD_INVALID_ARGUMENT);
        assert_eq!(
            spd_repair_base_crc(null_mut(), 0, null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_crc16_xmodem(null(), 0, null_mut()),
            SPD_INVALID_ARGUMENT
        );
    }
}

#[test]
fn nonempty_null_buffers_are_rejected_without_touching_outputs() {
    let mut header = SpdHeader {
        size_raw: 9,
        revision: 8,
        memory_type: 7,
        module_type_raw: 6,
    };
    let before = header;
    let mut check = SpdCrcCheck {
        stored: 1,
        calculated: 2,
    };
    let mut crc = 0xabcd;
    let mut image = [0x12; 4];
    let mut output = [0x34; 4];
    // SAFETY: Deliberately invalid null/length pairs are rejected before any
    // dereference. All other buffers are valid and disjoint.
    unsafe {
        assert_eq!(spd_header(null(), 1, &mut header), SPD_INVALID_ARGUMENT);
        assert_eq!(spd_base_crc(null(), 1, &mut check), SPD_INVALID_ARGUMENT);
        assert_eq!(
            spd_repair_base_crc(null_mut(), 1, &mut check),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(spd_crc16_xmodem(null(), 1, &mut crc), SPD_INVALID_ARGUMENT);
        assert_eq!(
            spd_image_read(null(), 1, 0, output.as_mut_ptr(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_read(image.as_ptr(), 4, 0, null_mut(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_write(null_mut(), 1, 0, output.as_ptr(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_write(image.as_mut_ptr(), 4, 0, null(), 1),
            SPD_INVALID_ARGUMENT
        );
    }
    assert_eq!(header, before);
    assert_eq!(
        check,
        SpdCrcCheck {
            stored: 1,
            calculated: 2
        }
    );
    assert_eq!(crc, 0xabcd);
    assert_eq!(image, [0x12; 4]);
    assert_eq!(output, [0x34; 4]);
}

#[test]
fn oversized_lengths_are_rejected_before_forming_any_slice() {
    let too_large = isize::MAX as usize + 1;
    let mut image = [0x12; 4];
    let mut output = [0x34; 4];
    let mut header = SpdHeader {
        size_raw: 9,
        revision: 8,
        memory_type: 7,
        module_type_raw: 6,
    };
    let mut check = SpdCrcCheck {
        stored: 1,
        calculated: 2,
    };
    let mut crc = 0xabcd;
    let io = SpdIo {
        context: null_mut(),
        read: None,
        write: None,
    };
    // SAFETY: Oversized declarations are intentionally invalid and must be
    // rejected before dereferencing the tiny, otherwise valid buffers.
    unsafe {
        assert_eq!(
            spd_header(image.as_ptr(), too_large, &mut header),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_base_crc(image.as_ptr(), too_large, &mut check),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_repair_base_crc(image.as_mut_ptr(), too_large, &mut check),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_crc16_xmodem(image.as_ptr(), too_large, &mut crc),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_read(image.as_ptr(), too_large, 0, output.as_mut_ptr(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_read(image.as_ptr(), 4, 0, output.as_mut_ptr(), too_large),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_write(image.as_mut_ptr(), too_large, 0, output.as_ptr(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_image_write(image.as_mut_ptr(), 4, 0, output.as_ptr(), too_large),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_io_read(&io, 0, output.as_mut_ptr(), too_large),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_io_write(&io, 0, output.as_ptr(), too_large),
            SPD_INVALID_ARGUMENT
        );
    }
    assert_eq!(image, [0x12; 4]);
    assert_eq!(output, [0x34; 4]);
    assert_eq!(header.size_raw, 9);
    assert_eq!(
        check,
        SpdCrcCheck {
            stored: 1,
            calculated: 2
        }
    );
    assert_eq!(crc, 0xabcd);
}

#[test]
fn overflow_and_out_of_range_edits_preserve_all_memory() {
    let mut image = [0x12; 4];
    let input = [0xa5; 2];
    let mut output = [0x34; 2];
    // SAFETY: All buffers are live and disjoint. Invalid ranges are checked
    // before output is modified, with no overflowing arithmetic.
    unsafe {
        for offset in [usize::MAX, 3, 5] {
            assert_eq!(
                spd_image_read(image.as_ptr(), 4, offset, output.as_mut_ptr(), 2),
                SPD_OUT_OF_BOUNDS
            );
            assert_eq!(
                spd_image_write(image.as_mut_ptr(), 4, offset, input.as_ptr(), 2),
                SPD_OUT_OF_BOUNDS
            );
            assert_eq!(image, [0x12; 4]);
            assert_eq!(output, [0x34; 2]);
        }
    }
}

#[test]
fn unsupported_and_truncated_crc_errors_preserve_outputs_and_image() {
    let mut image = [0u8; 128];
    image[2] = 0x7f;
    let mut check = SpdCrcCheck {
        stored: 0xabcd,
        calculated: 0x1234,
    };
    let sentinel = check;
    let snapshot = image;
    // SAFETY: Image and output are valid and disjoint for every declared size.
    unsafe {
        assert_eq!(
            spd_base_crc(image.as_ptr(), image.len(), &mut check),
            SPD_UNSUPPORTED_MEMORY_TYPE
        );
        assert_eq!(
            spd_repair_base_crc(image.as_mut_ptr(), image.len(), &mut check),
            SPD_UNSUPPORTED_MEMORY_TYPE
        );
        assert_eq!(check, sentinel);
        assert_eq!(image, snapshot);
        image[2] = 0x0c;
        let ddr4_snapshot = image;
        assert_eq!(spd_base_crc(image.as_ptr(), 127, &mut check), SPD_TRUNCATED);
        assert_eq!(
            spd_repair_base_crc(image.as_mut_ptr(), 127, &mut check),
            SPD_TRUNCATED
        );
        assert_eq!(check, sentinel);
        assert_eq!(image, ddr4_snapshot);
    }
}

fn manufacturing_sentinel() -> SpdManufacturing {
    SpdManufacturing {
        module_manufacturer: [0xa5; 2],
        location: 0xa5,
        date_raw: [0xa5; 2],
        serial: [0xa5; 4],
        part_number: [0xa5; 30],
        part_number_len: 0xa5,
        module_revision: 0xa5,
        dram_manufacturer: [0xa5; 2],
        dram_stepping: 0xa5,
    }
}

#[test]
fn typed_pointer_validation_precedes_slice_setup() {
    let mut image = [0u8; 512]; // Synthetic data, not a module dump.
    image[1..4].copy_from_slice(&[0x11, 0x0c, 0x03]);
    let snapshot = image;
    let mut timing = SpdTiming {
        picoseconds: 77,
        minimum_cycles: 88,
        has_minimum_cycles: 99,
    };
    let timing_before = timing;
    let mut manufacturing = manufacturing_sentinel();
    let manufacturing_before = manufacturing;
    let mut crc = SpdCrcCheck {
        stored: 11,
        calculated: 22,
    };
    let crc_before = crc;
    let serial = [1, 2, 3, 4];
    let valid_value = SpdTiming {
        picoseconds: 1250,
        minimum_cycles: 0,
        has_minimum_cycles: 0,
    };
    // SAFETY: Null/nonempty and oversized declarations must be rejected before
    // dereference. Other buffers/outputs are live, aligned and disjoint.
    unsafe {
        for (pointer, len) in [
            (null_mut(), 1),
            (image.as_mut_ptr(), isize::MAX as usize + 1),
        ] {
            assert_eq!(
                spd_timing_get(pointer, len, 1, &mut timing),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_timing_set(pointer, len, 1, valid_value),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_manufacturing_get(pointer, len, &mut manufacturing),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_part_number_set(pointer, len, null(), 0),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_serial_number_set(pointer, len, serial.as_ptr()),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(spd_module_crc(pointer, len, &mut crc), SPD_INVALID_ARGUMENT);
            assert_eq!(
                spd_repair_module_crc(pointer, len, &mut crc),
                SPD_INVALID_ARGUMENT
            );
        }
        assert_eq!(
            spd_timing_get(image.as_ptr(), 512, 1, null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_manufacturing_get(image.as_ptr(), 512, null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_module_crc(image.as_ptr(), 512, null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_repair_module_crc(image.as_mut_ptr(), 512, null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_serial_number_set(image.as_mut_ptr(), 512, null()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_part_number_set(image.as_mut_ptr(), 512, null(), 1),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_part_number_set(
                image.as_mut_ptr(),
                512,
                serial.as_ptr(),
                isize::MAX as usize + 1
            ),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(spd_timing_get(null(), 0, 1, &mut timing), SPD_TRUNCATED);
        assert_eq!(spd_timing_set(null_mut(), 0, 1, valid_value), SPD_TRUNCATED);
        assert_eq!(
            spd_manufacturing_get(null(), 0, &mut manufacturing),
            SPD_TRUNCATED
        );
        assert_eq!(spd_part_number_set(null_mut(), 0, null(), 0), SPD_TRUNCATED);
        assert_eq!(
            spd_serial_number_set(null_mut(), 0, serial.as_ptr()),
            SPD_TRUNCATED
        );
        assert_eq!(spd_module_crc(null(), 0, &mut crc), SPD_TRUNCATED);
        assert_eq!(
            spd_repair_module_crc(null_mut(), 0, &mut crc),
            SPD_TRUNCATED
        );
    }
    assert_eq!(image, snapshot);
    assert_eq!(timing, timing_before);
    assert_eq!(manufacturing, manufacturing_before);
    assert_eq!(crc, crc_before);
}

#[test]
fn native_timing_ids_and_presence_are_validated_without_mutation() {
    let mut image = [0u8; 1024]; // Synthetic DDR5 layout.
    image[1..4].copy_from_slice(&[0x10, 0x12, 0x02]);
    let before = image;
    let mut out = SpdTiming {
        picoseconds: 1,
        minimum_cycles: 2,
        has_minimum_cycles: 3,
    };
    let sentinel = out;
    // SAFETY: All calls use live disjoint buffers and outputs.
    unsafe {
        for field in [0, 24, u32::MAX] {
            assert_eq!(
                spd_timing_get(image.as_ptr(), image.len(), field, &mut out),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_timing_set(image.as_mut_ptr(), image.len(), field, sentinel),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(out, sentinel);
            assert_eq!(image, before);
        }
        for (minimum_cycles, has_minimum_cycles) in [(1, 0), (0, 2), (0, u32::MAX), (256, 1)] {
            let value = SpdTiming {
                picoseconds: 5000,
                minimum_cycles,
                has_minimum_cycles,
            };
            assert_eq!(
                spd_timing_set(image.as_mut_ptr(), image.len(), 14, value),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(image, before);
        }
        // Some(0) must survive conversion rather than becoming None.
        let zero_cycles = SpdTiming {
            picoseconds: 5000,
            minimum_cycles: 0,
            has_minimum_cycles: 1,
        };
        assert_eq!(
            spd_timing_set(image.as_mut_ptr(), image.len(), 14, zero_cycles),
            SPD_OK
        );
        assert_eq!(
            spd_timing_get(image.as_ptr(), image.len(), 14, &mut out),
            SPD_OK
        );
        assert_eq!(out, zero_cycles);
        let plain = SpdTiming {
            picoseconds: 295000,
            minimum_cycles: 0,
            has_minimum_cycles: 0,
        };
        assert_eq!(
            spd_timing_set(image.as_mut_ptr(), image.len(), 9, plain),
            SPD_OK
        );
        assert_eq!(
            spd_timing_get(image.as_ptr(), image.len(), 9, &mut out),
            SPD_OK
        );
        assert_eq!(out, plain); // Must clear both presence and cycle output words.
        assert_eq!(&image[42..44], &[0x27, 0x01]);
    }
}

#[test]
fn typed_core_errors_have_distinct_statuses_and_untouched_outputs() {
    let mut image = [0u8; 512]; // Synthetic DDR4 layout.
    image[1..4].copy_from_slice(&[0x11, 0x0c, 0x03]);
    let sentinel = SpdTiming {
        picoseconds: 11,
        minimum_cycles: 22,
        has_minimum_cycles: 33,
    };
    let mut out = sentinel;
    // SAFETY: Every declared input range is within the live allocation.
    unsafe {
        for (index, byte, status) in [
            (2, 0xff, SPD_UNSUPPORTED_MEMORY_TYPE),
            (1, 0x12, SPD_UNSUPPORTED_REVISION),
            (3, 0x02, SPD_UNSUPPORTED_MODULE_TYPE),
            (17, 1, SPD_UNSUPPORTED_TIMEBASE),
            (125, 0xff, SPD_INVALID_ENCODING),
        ] {
            let saved = image[index];
            image[index] = byte;
            assert_eq!(
                spd_timing_get(image.as_ptr(), image.len(), 1, &mut out),
                status
            );
            assert_eq!(out, sentinel);
            image[index] = saved;
        }
        assert_eq!(
            spd_timing_get(image.as_ptr(), image.len(), 6, &mut out),
            SPD_UNSUPPORTED_FIELD
        );
        assert_eq!(
            spd_timing_get(image.as_ptr(), 125, 1, &mut out),
            SPD_TRUNCATED
        );
        assert_eq!(out, sentinel);
        let before = image;
        let value = SpdTiming {
            picoseconds: 32003,
            minimum_cycles: 0,
            has_minimum_cycles: 0,
        };
        assert_eq!(
            spd_timing_set(image.as_mut_ptr(), image.len(), 1, value),
            SPD_VALUE_OUT_OF_RANGE
        );
        assert_eq!(image, before);
    }
}

#[test]
fn manufacturing_copy_owns_bytes_and_zeroes_only_unused_output_tail() {
    let mut image = [0u8; 1024]; // Synthetic layouts and raw manufacturing data.
                                 // SAFETY: Separate image, input and output allocations, within bounds.
    unsafe {
        for (revision, memory_type, module_type, start, width, end) in [
            (0x11, 0x0c, 0x03, 320, 20, 353),
            (0x10, 0x12, 0x02, 512, 30, 555),
        ] {
            image[1..4].copy_from_slice(&[revision, memory_type, module_type]);
            for (index, byte) in image[start..end].iter_mut().enumerate() {
                *byte = (index + 1) as u8;
            }
            let mut out = manufacturing_sentinel();
            let sentinel = out;
            assert_eq!(
                spd_manufacturing_get(image.as_ptr(), end - 1, &mut out),
                SPD_TRUNCATED
            );
            assert_eq!(out, sentinel);
            assert_eq!(spd_manufacturing_get(image.as_ptr(), end, &mut out), SPD_OK);
            assert_eq!(out.module_manufacturer, [1, 2]);
            assert_eq!(out.location, 3);
            assert_eq!(out.date_raw, [4, 5]);
            assert_eq!(out.serial, [6, 7, 8, 9]);
            assert_eq!(out.part_number_len as usize, width);
            assert_eq!(
                &out.part_number[..width],
                &image[start + 9..start + 9 + width]
            );
            assert!(out.part_number[width..].iter().all(|&byte| byte == 0));
            assert_eq!(out.module_revision, (10 + width) as u8);
            assert_eq!(
                out.dram_manufacturer,
                [(11 + width) as u8, (12 + width) as u8]
            );
            assert_eq!(out.dram_stepping, (13 + width) as u8);
            let serial = [0, 0xff, 0x80, 3];
            assert_eq!(
                spd_serial_number_set(image.as_mut_ptr(), start + 9, serial.as_ptr()),
                SPD_OK
            );
            assert_eq!(&image[start + 5..start + 9], &serial);
            let before = image;
            assert_eq!(
                spd_part_number_set(image.as_mut_ptr(), image.len(), [b'A', 0].as_ptr(), 2),
                SPD_INVALID_ENCODING
            );
            assert_eq!(image, before);
            assert_eq!(
                spd_part_number_set(image.as_mut_ptr(), start + 9 + width, null(), 0),
                SPD_OK
            );
            assert!(image[start + 9..start + 9 + width]
                .iter()
                .all(|&byte| byte == b' '));
        }
    }
}

#[test]
fn module_crc_errors_preserve_output_and_image_across_the_ffi() {
    let mut image = [0u8; 512]; // Synthetic supported DDR4 layout.
    image[1..4].copy_from_slice(&[0x13, 0x0c, 0x03]);
    let mut out = SpdCrcCheck {
        stored: 0xabcd,
        calculated: 0x1234,
    };
    let sentinel = out;
    let before = image;
    // SAFETY: All declared ranges are within the live image; output disjoint.
    unsafe {
        assert_eq!(spd_module_crc(image.as_ptr(), 255, &mut out), SPD_TRUNCATED);
        assert_eq!(
            spd_repair_module_crc(image.as_mut_ptr(), 255, &mut out),
            SPD_TRUNCATED
        );
        assert_eq!(out, sentinel);
        assert_eq!(image, before);
        assert_eq!(
            spd_repair_module_crc(image.as_mut_ptr(), image.len(), &mut out),
            SPD_OK
        );
        assert_eq!(out.stored, out.calculated);
        image[128] = 0x42;
        assert_eq!(
            spd_module_crc(image.as_ptr(), image.len(), &mut out),
            SPD_OK
        );
        assert_ne!(out.stored, out.calculated);
        image[1..4].copy_from_slice(&[0x10, 0x12, 0x02]);
        let unsupported = image;
        let previous = out;
        assert_eq!(
            spd_module_crc(image.as_ptr(), image.len(), &mut out),
            SPD_UNSUPPORTED_FIELD
        );
        assert_eq!(
            spd_repair_module_crc(image.as_mut_ptr(), image.len(), &mut out),
            SPD_UNSUPPORTED_FIELD
        );
        assert_eq!(image, unsupported);
        assert_eq!(out, previous);
    }
}

#[test]
fn legacy_native_outputs_are_atomic_and_metadata_keeps_its_real_width() {
    let mut image = [0u8; 256]; // Sparse synthetic input, not a real module dump.
    let identity_sentinel = SpdIdentityInfo {
        memory_type: 0xa5,
        revision: 0xa5,
        module_type: 0xa5,
        has_module_type: 0xa5,
    };
    let manufacturing_sentinel = SpdLegacyManufacturingInfo {
        manufacturer: [0xa5; 8],
        manufacturer_len: 0xa5,
        location: 0xa5,
        date_raw: [0xa5; 2],
        serial: [0xa5; 4],
        part_number: [0xa5; 18],
        revision: [0xa5; 2],
    };
    // SAFETY: Declared ranges fit live allocations; outputs are disjoint.
    unsafe {
        for (memory_type, revision, end, manufacturer_start, manufacturer_len) in [
            (0x01, 1, 99, 64, 8),
            (0x02, 1, 99, 64, 8),
            (0x04, 1, 99, 64, 8),
            (0x07, 0x10, 99, 64, 8),
            (0x08, 0x12, 99, 64, 8),
            (0x0b, 0x11, 148, 117, 2),
        ] {
            image.fill(0);
            image[1] = if memory_type == 0x0b { revision } else { 0xa5 };
            image[2] = memory_type;
            image[3] = 0x03;
            image[20] = 0x04;
            image[62] = revision;
            for (index, byte) in image[manufacturer_start..end].iter_mut().enumerate() {
                *byte = (index + 1) as u8;
            }
            let mut identity = identity_sentinel;
            assert_eq!(
                spd_identity(image.as_ptr(), image.len(), &mut identity),
                SPD_OK
            );
            assert_eq!(identity.memory_type, memory_type);
            assert_eq!(identity.revision, revision);
            assert_eq!(identity.has_module_type, u8::from(memory_type >= 0x08));
            assert_eq!(
                identity.module_type,
                match memory_type {
                    0x08 => 4,
                    0x0b => 3,
                    _ => 0,
                }
            );
            let mut manufacturing = manufacturing_sentinel;
            for len in 0..end {
                assert_eq!(
                    spd_legacy_manufacturing(image.as_ptr(), len, &mut manufacturing),
                    SPD_TRUNCATED
                );
                assert_eq!(manufacturing, manufacturing_sentinel);
            }
            assert_eq!(
                spd_legacy_manufacturing(image.as_ptr(), end, &mut manufacturing),
                SPD_OK
            );
            assert_eq!(manufacturing.manufacturer_len as usize, manufacturer_len);
            assert_eq!(
                &manufacturing.manufacturer[..manufacturer_len],
                &image[manufacturer_start..manufacturer_start + manufacturer_len]
            );
            assert!(manufacturing.manufacturer[manufacturer_len..]
                .iter()
                .all(|b| *b == 0));
            let part_start = if memory_type == 0x0b { 128 } else { 73 };
            let revision_start = if memory_type == 0x0b { 146 } else { 91 };
            assert_eq!(
                manufacturing.part_number,
                image[part_start..part_start + 18]
            );
            assert_eq!(
                manufacturing.revision,
                image[revision_start..revision_start + 2]
            );
        }
    }
}

#[test]
fn legacy_native_pointer_errors_and_truncated_repairs_do_not_touch_outputs() {
    let mut image = [0u8; 99];
    image[2] = 0x01;
    image[9] = 60;
    image[62] = 1;
    let before = image;
    let mut identity = SpdIdentityInfo {
        memory_type: 1,
        revision: 2,
        module_type: 3,
        has_module_type: 4,
    };
    let identity_before = identity;
    let mut manufacturing = SpdLegacyManufacturingInfo {
        manufacturer: [9; 8],
        manufacturer_len: 8,
        location: 7,
        date_raw: [6; 2],
        serial: [5; 4],
        part_number: [4; 18],
        revision: [3; 2],
    };
    let manufacturing_before = manufacturing;
    let mut check = SpdChecksumCheck {
        stored: 0xa5,
        calculated: 0x5a,
    };
    let check_before = check;
    // SAFETY: Invalid pointer/length pairs must be rejected before dereference;
    // all other storage is live, aligned and disjoint.
    unsafe {
        for (pointer, len) in [
            (null_mut(), 1),
            (image.as_mut_ptr(), isize::MAX as usize + 1),
        ] {
            assert_eq!(
                spd_identity(pointer, len, &mut identity),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(
                spd_legacy_manufacturing(pointer, len, &mut manufacturing),
                SPD_INVALID_ARGUMENT
            );
            assert_eq!(spd_checksum(pointer, len, &mut check), SPD_INVALID_ARGUMENT);
            assert_eq!(
                spd_repair_checksum(pointer, len, &mut check),
                SPD_INVALID_ARGUMENT
            );
        }
        assert_eq!(
            spd_identity(image.as_ptr(), image.len(), null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_legacy_manufacturing(image.as_ptr(), image.len(), null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_checksum(image.as_ptr(), image.len(), null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(
            spd_repair_checksum(image.as_mut_ptr(), image.len(), null_mut()),
            SPD_INVALID_ARGUMENT
        );
        assert_eq!(spd_identity(null(), 0, &mut identity), SPD_TRUNCATED);
        assert_eq!(
            spd_legacy_manufacturing(null(), 0, &mut manufacturing),
            SPD_TRUNCATED
        );
        assert_eq!(spd_checksum(null(), 0, &mut check), SPD_TRUNCATED);
        assert_eq!(
            spd_repair_checksum(null_mut(), 0, &mut check),
            SPD_TRUNCATED
        );
        for len in 0..64 {
            assert_eq!(
                spd_repair_checksum(image.as_mut_ptr(), len, &mut check),
                SPD_TRUNCATED
            );
            assert_eq!(image, before);
            assert_eq!(check, check_before);
        }
        assert_eq!(identity, identity_before);
        assert_eq!(manufacturing, manufacturing_before);
        assert_eq!(
            spd_repair_checksum(image.as_mut_ptr(), image.len(), &mut check),
            SPD_OK
        );
        assert_eq!(check.stored, 62); // type1 + tRAC60 + revision1.
        assert_eq!(check.calculated, 62);
        assert_eq!(image[63], 62);
        for (index, byte) in image.iter().enumerate() {
            if index != 63 {
                assert_eq!(*byte, before[index]);
            }
        }
    }
}
