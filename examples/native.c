/* SPDX-License-Identifier: Apache-2.0 */
/* Synthetic in-memory SPD data only: no hardware reader or real module dump. */
#include "libspd.h"

#include <stdio.h>
#include <string.h>

/* Unlike assert(), checks remain active when compiled with NDEBUG. */
#define CHECK(condition) do { \
    if (!(condition)) { \
        fprintf(stderr, "native.c:%d: %s\n", __LINE__, #condition); \
        return 1; \
    } \
} while (0)

struct synthetic_backend {
    uint8_t bytes[512];
    unsigned reads;
    unsigned writes;
    int fail_read;
    int fail_write;
};

static int32_t synthetic_read(void *context, size_t offset,
                              uint8_t *output, size_t len)
{
    struct synthetic_backend *backend = (struct synthetic_backend *)context;
    ++backend->reads;
    if (offset > sizeof backend->bytes || len > sizeof backend->bytes - offset)
        return -1;
    if (backend->fail_read) {
        if (len != 0)
            output[0] = backend->bytes[offset];
        return -7; /* Demonstrate failure after partial output. */
    }
    if (len != 0)
        memcpy(output, backend->bytes + offset, len);
    return 0;
}

static int32_t synthetic_write(void *context, size_t offset,
                               const uint8_t *input, size_t len)
{
    struct synthetic_backend *backend = (struct synthetic_backend *)context;
    ++backend->writes;
    if (offset > sizeof backend->bytes || len > sizeof backend->bytes - offset)
        return -1;
    if (backend->fail_write) {
        if (len != 0)
            backend->bytes[offset] = input[0];
        return 23; /* No rollback: even a nonzero positive status is failure. */
    }
    if (len != 0)
        memcpy(backend->bytes + offset, input, len);
    return 0;
}

/* Six old generations, entirely synthetic selected fields, not module dumps. */
static int legacy_matrix(void)
{
    const uint8_t types[] = {0x01, 0x02, 0x04, 0x07, 0x08, 0x0b};
    const uint8_t revisions[] = {1, 1, 1, 0x10, 0x12, 0x11};
    const uint32_t initial[] = {60000, 60000, 20000, 18000, 12500, 1071};
    const uint32_t edited[] = {65000, 65000, 21000, 18250, 12750, 1072};
    size_t generation;
    for (generation = 0; generation < sizeof types; ++generation) {
        unsigned coverage;
        for (coverage = 0; coverage < (generation == 5 ? 2u : 1u); ++coverage) {
            uint8_t image[256] = {0};
            uint8_t snapshot[256];
            const uint8_t serial[] = {0x01, 0x80, 0xff, 0x37};
            const uint8_t part[] = "SYNTHETIC-LEGACY";
            const uint32_t field = generation < 2 ? SPD_TIMING_TRAC
                                   : generation == 5 ? SPD_TIMING_TCK_MIN : SPD_TIMING_TRP;
            spd_header_info raw;
            spd_identity_info identity;
            spd_legacy_manufacturing_info manufacturing;
            spd_timing decoded;
            spd_timing value = {0, 0, 0};
            spd_checksum_check sum;
            spd_crc_check crc;
            image[0] = coverage ? 0x80 : 0;
            image[1] = generation == 5 ? revisions[generation] : 0xa5;
            image[2] = types[generation];
            image[3] = 0x03;
            image[20] = 0x04;
            image[62] = revisions[generation];
            if (generation < 2) {
                image[9] = 60;
                image[10] = 15;
            } else if (generation == 5) {
                image[9] = 0x11;
                image[10] = 1;
                image[11] = 8;
                image[12] = 9;
                image[34] = 0xca; /* 9*125 -54 =1071ps. */
            } else {
                image[27] = generation == 2 ? 20 : generation == 3 ? 0x48 : 0x32;
            }
            CHECK(spd_header(image, sizeof image, &raw) == SPD_OK);
            CHECK(raw.revision == image[1] && raw.module_type_raw == image[3]);
            CHECK(spd_identity(image, sizeof image, &identity) == SPD_OK);
            CHECK(identity.memory_type == types[generation]
                  && identity.revision == revisions[generation]);
            CHECK(identity.has_module_type == (generation >= 4));
            CHECK(identity.module_type == (generation == 4 ? 4 : generation == 5 ? 3 : 0));
            CHECK(spd_timing_get(image, sizeof image, field, &decoded) == SPD_OK);
            CHECK(decoded.picoseconds == initial[generation] && decoded.has_minimum_cycles == 0);
            if (generation == 5) {
                CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
                CHECK(spd_checksum(image, sizeof image, &sum) == SPD_UNSUPPORTED_FIELD);
            } else {
                unsigned independent_sum = 0;
                size_t byte;
                for (byte = 0; byte < 63; ++byte)
                    independent_sum += image[byte];
                CHECK(spd_repair_checksum(image, sizeof image, &sum) == SPD_OK);
                CHECK(sum.stored == (uint8_t)independent_sum && sum.stored == sum.calculated);
                CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_UNSUPPORTED_FIELD);
            }
            value.picoseconds = edited[generation];
            CHECK(spd_timing_set(image, sizeof image, field, value) == SPD_OK);
            CHECK(spd_timing_get(image, sizeof image, field, &decoded) == SPD_OK);
            CHECK(decoded.picoseconds == value.picoseconds);
            if (generation == 5) {
                CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_OK);
                CHECK(crc.stored != crc.calculated);
                CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
            } else {
                CHECK(spd_checksum(image, sizeof image, &sum) == SPD_OK);
                CHECK(sum.stored != sum.calculated);
                CHECK(spd_repair_checksum(image, sizeof image, &sum) == SPD_OK);
            }
            memcpy(snapshot, image, sizeof image);
            value.minimum_cycles = 1;
            value.has_minimum_cycles = 1;
            CHECK(spd_timing_set(image, sizeof image, field, value) == SPD_VALUE_OUT_OF_RANGE);
            CHECK(memcmp(snapshot, image, sizeof image) == 0);
            CHECK(spd_part_number_set(image, sizeof image, part, sizeof part - 1) == SPD_OK);
            CHECK(spd_serial_number_set(image, sizeof image, serial) == SPD_OK);
            CHECK(spd_legacy_manufacturing(image, sizeof image, &manufacturing) == SPD_OK);
            CHECK(manufacturing.manufacturer_len == (generation == 5 ? 2 : 8));
            CHECK(memcmp(manufacturing.part_number, part, sizeof part - 1) == 0);
            CHECK(manufacturing.part_number[17] == ' ');
            CHECK(memcmp(manufacturing.serial, serial, sizeof serial) == 0);
            if (generation == 5) {
                CHECK(manufacturing.manufacturer[2] == 0 && manufacturing.manufacturer[7] == 0);
                CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
                image[120] ^= 1; /* Metadata is covered only when bit7 is clear. */
                CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_OK);
                CHECK((crc.stored == crc.calculated) == (coverage != 0));
                CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
                CHECK(crc.stored == crc.calculated);
            } else {
                CHECK(spd_checksum(image, sizeof image, &sum) == SPD_OK);
                CHECK(sum.stored == sum.calculated); /* Metadata outside checksum. */
            }
        }
    }
    return 0;
}

int main(void)
{
    uint8_t image[512] = {0};
    uint8_t before[512];
    const uint8_t edit = 0xa5;
    const uint8_t write_bytes[2] = {0x31, 0x72};
    uint8_t output[4] = {0};
    uint16_t standalone = 0xffff;
    uint16_t old_crc;
    spd_header_info header;
    spd_crc_check crc;
    struct synthetic_backend backend = {{0}, 0, 0, 0, 0};
    spd_io io = {&backend, synthetic_read, synthetic_write};
    spd_io absent = {NULL, NULL, NULL};
    unsigned calls;

    CHECK(legacy_matrix() == 0);
    image[0] = 0x23;
    image[1] = 0x11;
    image[2] = 0x0c; /* DDR4, intentionally synthetic, not a validated SPD. */
    image[3] = 0x03;
    CHECK(spd_abi_version() == 1);
    CHECK(spd_header(image, sizeof image, &header) == SPD_OK);
    CHECK(header.size_raw == 0x23 && header.revision == 0x11);
    CHECK(header.memory_type == 0x0c && header.module_type_raw == 0x03);

    CHECK(spd_crc16_xmodem((const uint8_t *)"123456789", 9, &standalone) == SPD_OK);
    CHECK(standalone == 0x31c3);
    CHECK(spd_crc16_xmodem(NULL, 0, &standalone) == SPD_OK && standalone == 0);
    CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated);
    CHECK(image[126] == (uint8_t)crc.stored);
    CHECK(image[127] == (uint8_t)(crc.stored >> 8));
    old_crc = crc.stored;

    CHECK(spd_image_write(image, sizeof image, 5, &edit, 1) == SPD_OK);
    CHECK(spd_image_read(image, sizeof image, 5, output, 1) == SPD_OK);
    CHECK(output[0] == edit);
    CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_OK);
    CHECK(crc.stored == old_crc && crc.stored != crc.calculated);
    CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated); /* Repair was explicit, not part of edit. */

    /* Typed fields on synthetic, supported DDR4 layout; CRC remains explicit. */
    {
        spd_timing timing = {13750, 0, 0};
        spd_timing decoded = {0, 0, 0};
        spd_manufacturing manufacturing;
        const uint8_t serial[4] = {0x00, 0x80, 0xff, 0x17};
        const uint8_t part[] = "SYNTHETIC-DDR4";
        uint8_t module_edit = 0x42;
        old_crc = crc.stored;
        CHECK(spd_timing_set(image, sizeof image, SPD_TIMING_TAA, timing) == SPD_OK);
        CHECK(spd_timing_get(image, sizeof image, SPD_TIMING_TAA, &decoded) == SPD_OK);
        CHECK(decoded.picoseconds == 13750 && decoded.has_minimum_cycles == 0
              && decoded.minimum_cycles == 0);
        CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == old_crc && crc.stored != crc.calculated);
        CHECK(spd_repair_base_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
        memcpy(before, image, sizeof image);
        timing.has_minimum_cycles = 1; /* Native-valid but not a DDR4 value. */
        CHECK(spd_timing_set(image, sizeof image, SPD_TIMING_TAA, timing)
              == SPD_VALUE_OUT_OF_RANGE);
        CHECK(memcmp(before, image, sizeof image) == 0);
        CHECK(spd_timing_get(image, sizeof image, SPD_TIMING_TRAS, &decoded)
              == SPD_UNSUPPORTED_FIELD);
        image[1] = 0x12;
        CHECK(spd_timing_get(image, sizeof image, SPD_TIMING_TAA, &decoded)
              == SPD_UNSUPPORTED_REVISION);
        image[1] = 0x11;

        CHECK(spd_part_number_set(image, sizeof image, part, sizeof part - 1) == SPD_OK);
        CHECK(spd_serial_number_set(image, sizeof image, serial) == SPD_OK);
        CHECK(spd_manufacturing_get(image, sizeof image, &manufacturing) == SPD_OK);
        CHECK(manufacturing.part_number_len == 20);
        CHECK(memcmp(manufacturing.part_number, part, sizeof part - 1) == 0);
        CHECK(manufacturing.part_number[19] == ' ' && manufacturing.part_number[20] == 0);
        CHECK(memcmp(manufacturing.serial, serial, sizeof serial) == 0);
        CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated); /* Manufacturing is outside base CRC. */

        CHECK(spd_repair_module_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
        old_crc = crc.stored;
        CHECK(spd_image_write(image, sizeof image, 128, &module_edit, 1) == SPD_OK);
        CHECK(spd_module_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == old_crc && crc.stored != crc.calculated);
        CHECK(spd_repair_module_crc(image, sizeof image, &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
    }

    /* DDR5 direct-ps, ns refresh conversion, and independently present nCK. */
    {
        uint8_t ddr5[1024] = {0};
        uint8_t snapshot[1024];
        spd_timing timing = {295000, 0, 0};
        spd_timing decoded = {0, 0, 0};
        spd_manufacturing manufacturing;
        const uint8_t part[] = "SYNTHETIC-DDR5";
        const uint8_t serial[4] = {1, 2, 3, 4};
        ddr5[1] = 0x10;
        ddr5[2] = 0x12;
        ddr5[3] = 0x02;
        CHECK(spd_repair_base_crc(ddr5, sizeof ddr5, &crc) == SPD_OK);
        old_crc = crc.stored;
        CHECK(spd_timing_set(ddr5, sizeof ddr5, SPD_TIMING_TRFC1, timing) == SPD_OK);
        CHECK(ddr5[42] == 0x27 && ddr5[43] == 0x01); /* 295 ns. */
        CHECK(spd_timing_get(ddr5, sizeof ddr5, SPD_TIMING_TRFC1, &decoded) == SPD_OK);
        CHECK(decoded.picoseconds == 295000 && decoded.has_minimum_cycles == 0);
        timing.picoseconds = 5000;
        timing.minimum_cycles = 8;
        timing.has_minimum_cycles = 1;
        CHECK(spd_timing_set(ddr5, sizeof ddr5, SPD_TIMING_TRRD_L, timing) == SPD_OK);
        CHECK(spd_timing_get(ddr5, sizeof ddr5, SPD_TIMING_TRRD_L, &decoded) == SPD_OK);
        CHECK(decoded.picoseconds == 5000 && decoded.minimum_cycles == 8
              && decoded.has_minimum_cycles == 1);
        CHECK(spd_base_crc(ddr5, sizeof ddr5, &crc) == SPD_OK);
        CHECK(crc.stored == old_crc && crc.stored != crc.calculated);
        CHECK(spd_repair_base_crc(ddr5, sizeof ddr5, &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
        memcpy(snapshot, ddr5, sizeof ddr5);
        timing.has_minimum_cycles = 2;
        CHECK(spd_timing_set(ddr5, sizeof ddr5, SPD_TIMING_TRRD_L, timing)
              == SPD_INVALID_ARGUMENT);
        CHECK(memcmp(snapshot, ddr5, sizeof ddr5) == 0);
        timing.picoseconds = 295001;
        timing.minimum_cycles = 0;
        timing.has_minimum_cycles = 0;
        CHECK(spd_timing_set(ddr5, sizeof ddr5, SPD_TIMING_TRFC1, timing)
              == SPD_VALUE_OUT_OF_RANGE);
        CHECK(memcmp(snapshot, ddr5, sizeof ddr5) == 0);
        CHECK(spd_timing_get(ddr5, sizeof ddr5, SPD_TIMING_TRFC4, &decoded)
              == SPD_UNSUPPORTED_FIELD);
        CHECK(spd_module_crc(ddr5, sizeof ddr5, &crc) == SPD_UNSUPPORTED_FIELD);
        CHECK(spd_part_number_set(ddr5, sizeof ddr5, part, sizeof part - 1) == SPD_OK);
        CHECK(spd_serial_number_set(ddr5, sizeof ddr5, serial) == SPD_OK);
        CHECK(spd_manufacturing_get(ddr5, sizeof ddr5, &manufacturing) == SPD_OK);
        CHECK(manufacturing.part_number_len == 30 && manufacturing.part_number[29] == ' ');
        CHECK(memcmp(manufacturing.part_number, part, sizeof part - 1) == 0);
        CHECK(memcmp(manufacturing.serial, serial, sizeof serial) == 0);
    }

    memcpy(before, image, sizeof image);
    CHECK(spd_image_write(image, sizeof image, SIZE_MAX, &edit, 1) == SPD_OUT_OF_BOUNDS);
    CHECK(memcmp(before, image, sizeof image) == 0);
    output[0] = 0x7e;
    CHECK(spd_image_read(image, sizeof image, sizeof image, output, 1) == SPD_OUT_OF_BOUNDS);
    CHECK(output[0] == 0x7e);
    CHECK(spd_image_read(image, sizeof image, sizeof image, NULL, 0) == SPD_OK);
    CHECK(spd_header(NULL, 1, &header) == SPD_INVALID_ARGUMENT);
    CHECK(spd_header(image, sizeof image, NULL) == SPD_INVALID_ARGUMENT);
    CHECK(spd_header(image, (size_t)PTRDIFF_MAX + 1u, &header) == SPD_INVALID_ARGUMENT);
    header.size_raw = 0x99;
    CHECK(spd_header(image, 3, &header) == SPD_TRUNCATED);
    CHECK(header.size_raw == 0x99);
    CHECK(spd_base_crc(image, 127, &crc) == SPD_TRUNCATED);
    image[2] = 0xff;
    CHECK(spd_header(image, sizeof image, &header) == SPD_OK && header.memory_type == 0xff);
    crc.stored = 0xabcd;
    crc.calculated = 0x1234;
    CHECK(spd_base_crc(image, sizeof image, &crc) == SPD_UNSUPPORTED_MEMORY_TYPE);
    CHECK(crc.stored == 0xabcd && crc.calculated == 0x1234);
    image[2] = 0x0c;

    memcpy(backend.bytes, image, sizeof image);
    CHECK(spd_io_read(&io, 0, output, sizeof output) == SPD_OK);
    CHECK(backend.reads == 1 && memcmp(output, image, sizeof output) == 0);
    CHECK(spd_io_write(&io, 8, write_bytes, sizeof write_bytes) == SPD_OK);
    CHECK(backend.writes == 1 && memcmp(backend.bytes + 8, write_bytes, 2) == 0);
    CHECK(spd_io_read(&io, 0, NULL, 0) == SPD_OK && backend.reads == 2);
    CHECK(spd_io_write(&io, 0, NULL, 0) == SPD_OK && backend.writes == 2);
    calls = backend.reads;
    CHECK(spd_io_read(&io, SIZE_MAX, output, 1) == SPD_OUT_OF_BOUNDS);
    CHECK(backend.reads == calls);
    CHECK(spd_io_read(&absent, 0, output, 1) == SPD_INVALID_ARGUMENT);
    CHECK(spd_io_write(NULL, 0, write_bytes, 1) == SPD_INVALID_ARGUMENT);

    backend.fail_read = 1;
    output[0] = 0;
    output[1] = 0xee;
    CHECK(spd_io_read(&io, 0, output, 2) == SPD_CALLBACK_ERROR);
    CHECK(backend.reads == calls + 1 && output[0] == image[0] && output[1] == 0xee);
    backend.fail_write = 1;
    calls = backend.writes;
    CHECK(spd_io_write(&io, 20, write_bytes, 2) == SPD_CALLBACK_ERROR);
    CHECK(backend.writes == calls + 1);
    CHECK(backend.bytes[20] == write_bytes[0] && backend.bytes[21] == 0);

    puts("C ABI: synthetic FPM/EDO/SDR/DDR1-5 fields, explicit integrity repair and callbacks passed");
    return 0;
}
