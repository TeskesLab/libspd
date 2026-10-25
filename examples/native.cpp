// SPDX-License-Identifier: Apache-2.0
// Direct C ABI calls from C++17; no wrapper layer and no real hardware/data.
#include "libspd.h"

#include <array>
#include <cstdio>
#include <cstring>
#include <limits>

#define CHECK(condition) do { \
    if (!(condition)) { \
        std::fprintf(stderr, "native.cpp:%d: %s\n", __LINE__, #condition); \
        return 1; \
    } \
} while (0)

static_assert(sizeof(spd_header_info) == 4, "ABI header layout");
static_assert(sizeof(spd_crc_check) == 4, "ABI CRC layout");
static_assert(sizeof(spd_timing) == 12, "ABI timing layout");
static_assert(sizeof(spd_manufacturing) == 44, "ABI manufacturing layout");
static_assert(sizeof(spd_identity_info) == 4, "ABI identity layout");
static_assert(sizeof(spd_checksum_check) == 2, "ABI checksum layout");
static_assert(sizeof(spd_legacy_manufacturing_info) == 36, "ABI legacy manufacturing layout");
static_assert(offsetof(spd_timing, has_minimum_cycles) == 8, "ABI timing presence");
static_assert(offsetof(spd_manufacturing, part_number) == 9, "ABI part number");

struct SyntheticBackend {
    std::array<uint8_t, 1024> bytes{};
    unsigned reads = 0;
    unsigned writes = 0;
    bool fail_read = false;
    bool fail_write = false;
};

extern "C" int32_t synthetic_cpp_read(void *context, size_t offset,
                                       uint8_t *output, size_t len) noexcept
{
    auto &backend = *static_cast<SyntheticBackend *>(context);
    ++backend.reads;
    if (offset > backend.bytes.size() || len > backend.bytes.size() - offset)
        return -1;
    if (backend.fail_read) {
        if (len != 0)
            output[0] = backend.bytes[offset];
        return -5;
    }
    if (len != 0)
        std::memcpy(output, backend.bytes.data() + offset, len);
    return 0;
}

extern "C" int32_t synthetic_cpp_write(void *context, size_t offset,
                                        const uint8_t *input, size_t len) noexcept
{
    auto &backend = *static_cast<SyntheticBackend *>(context);
    ++backend.writes;
    if (offset > backend.bytes.size() || len > backend.bytes.size() - offset)
        return -1;
    if (backend.fail_write) {
        if (len != 0)
            backend.bytes[offset] = input[0];
        return 17; // A failed operation can still have partially changed storage.
    }
    if (len != 0)
        std::memcpy(backend.bytes.data() + offset, input, len);
    return 0;
}

// Six old generations; all raw unspecified fields are synthetic zero bytes.
static int legacy_matrix()
{
    constexpr std::array<uint8_t, 6> types{0x01, 0x02, 0x04, 0x07, 0x08, 0x0b};
    constexpr std::array<uint8_t, 6> revisions{1, 1, 1, 0x10, 0x12, 0x11};
    constexpr std::array<uint32_t, 6> initial{60000, 60000, 20000, 18000, 12500, 1071};
    constexpr std::array<uint32_t, 6> edited{65000, 65000, 21000, 18250, 12750, 1072};
    for (size_t generation = 0; generation < types.size(); ++generation) {
        for (unsigned coverage = 0; coverage < (generation == 5 ? 2u : 1u); ++coverage) {
            std::array<uint8_t, 256> image{};
            const std::array<uint8_t, 4> serial{0x01, 0x80, 0xff, 0x37};
            const uint8_t part[] = "SYNTHETIC-LEGACY";
            const uint32_t field = generation < 2 ? SPD_TIMING_TRAC
                                   : generation == 5 ? SPD_TIMING_TCK_MIN : SPD_TIMING_TRP;
            spd_header_info raw{};
            spd_identity_info identity{};
            spd_legacy_manufacturing_info manufacturing{};
            spd_timing decoded{};
            spd_timing value{edited[generation], 0, 0};
            spd_checksum_check sum{};
            spd_crc_check crc{};
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
                image[34] = 0xca; // 9*125 -54 =1071ps.
            } else {
                image[27] = generation == 2 ? 20 : generation == 3 ? 0x48 : 0x32;
            }
            CHECK(spd_header(image.data(), image.size(), &raw) == SPD_OK);
            CHECK(raw.revision == image[1] && raw.module_type_raw == image[3]);
            CHECK(spd_identity(image.data(), image.size(), &identity) == SPD_OK);
            CHECK(identity.memory_type == types[generation]
                  && identity.revision == revisions[generation]);
            CHECK(identity.has_module_type == (generation >= 4));
            CHECK(identity.module_type == (generation == 4 ? 4 : generation == 5 ? 3 : 0));
            CHECK(spd_timing_get(image.data(), image.size(), field, &decoded) == SPD_OK);
            CHECK(decoded.picoseconds == initial[generation] && decoded.has_minimum_cycles == 0);
            if (generation == 5) {
                CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
                CHECK(spd_checksum(image.data(), image.size(), &sum) == SPD_UNSUPPORTED_FIELD);
            } else {
                unsigned independent_sum = 0;
                for (size_t byte = 0; byte < 63; ++byte)
                    independent_sum += image[byte];
                CHECK(spd_repair_checksum(image.data(), image.size(), &sum) == SPD_OK);
                CHECK(sum.stored == static_cast<uint8_t>(independent_sum)
                      && sum.stored == sum.calculated);
                CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_UNSUPPORTED_FIELD);
            }
            CHECK(spd_timing_set(image.data(), image.size(), field, value) == SPD_OK);
            CHECK(spd_timing_get(image.data(), image.size(), field, &decoded) == SPD_OK);
            CHECK(decoded.picoseconds == value.picoseconds);
            if (generation == 5) {
                CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_OK);
                CHECK(crc.stored != crc.calculated);
                CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
            } else {
                CHECK(spd_checksum(image.data(), image.size(), &sum) == SPD_OK);
                CHECK(sum.stored != sum.calculated);
                CHECK(spd_repair_checksum(image.data(), image.size(), &sum) == SPD_OK);
            }
            const auto snapshot = image;
            value.minimum_cycles = 1;
            value.has_minimum_cycles = 1;
            CHECK(spd_timing_set(image.data(), image.size(), field, value) == SPD_VALUE_OUT_OF_RANGE);
            CHECK(image == snapshot);
            CHECK(spd_part_number_set(image.data(), image.size(), part, sizeof part - 1) == SPD_OK);
            CHECK(spd_serial_number_set(image.data(), image.size(), serial.data()) == SPD_OK);
            CHECK(spd_legacy_manufacturing(image.data(), image.size(), &manufacturing) == SPD_OK);
            CHECK(manufacturing.manufacturer_len == (generation == 5 ? 2 : 8));
            CHECK(std::memcmp(manufacturing.part_number, part, sizeof part - 1) == 0);
            CHECK(manufacturing.part_number[17] == ' ');
            CHECK(std::memcmp(manufacturing.serial, serial.data(), serial.size()) == 0);
            if (generation == 5) {
                CHECK(manufacturing.manufacturer[2] == 0 && manufacturing.manufacturer[7] == 0);
                CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
                image[120] ^= 1; // Included only when byte0 bit7 is clear.
                CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_OK);
                CHECK((crc.stored == crc.calculated) == (coverage != 0));
                CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
                CHECK(crc.stored == crc.calculated);
            } else {
                CHECK(spd_checksum(image.data(), image.size(), &sum) == SPD_OK);
                CHECK(sum.stored == sum.calculated);
            }
        }
    }
    return 0;
}

int main()
{
    CHECK(legacy_matrix() == 0);
    std::array<uint8_t, 1024> image{};
    image[0] = 0x30;
    image[1] = 0x10;
    image[2] = 0x12; // Synthetic DDR5; not a claim of a complete valid SPD dump.
    image[3] = 0x02;
    spd_header_info header{};
    spd_crc_check crc{};
    uint16_t standalone = 0;
    CHECK(spd_abi_version() == 1);
    CHECK(spd_header(image.data(), image.size(), &header) == SPD_OK);
    CHECK(header.size_raw == 0x30 && header.revision == 0x10);
    CHECK(header.memory_type == 0x12 && header.module_type_raw == 0x02);
    CHECK(spd_crc16_xmodem(reinterpret_cast<const uint8_t *>("123456789"),
                             9, &standalone) == SPD_OK);
    CHECK(standalone == 0x31c3);
    CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated);
    CHECK(image[510] == static_cast<uint8_t>(crc.stored));
    CHECK(image[511] == static_cast<uint8_t>(crc.stored >> 8));
    const auto old_crc = crc.stored;

    const uint8_t replacement = 0x5a;
    CHECK(spd_image_write(image.data(), image.size(), 500, &replacement, 1) == SPD_OK);
    uint8_t readback = 0;
    CHECK(spd_image_read(image.data(), image.size(), 500, &readback, 1) == SPD_OK);
    CHECK(readback == replacement);
    CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == old_crc && crc.stored != crc.calculated);
    CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated); // Explicit repair, never an edit side effect.

    // Selected fields on synthetic DDR5 data; ns units are not raw ps words.
    const auto typed_crc = crc.stored;
    spd_timing timing{295000, 0, 0};
    spd_timing decoded{};
    CHECK(spd_timing_set(image.data(), image.size(), SPD_TIMING_TRFC1, timing) == SPD_OK);
    CHECK(image[42] == 0x27 && image[43] == 0x01); // 295 ns on image.
    CHECK(spd_timing_get(image.data(), image.size(), SPD_TIMING_TRFC1, &decoded) == SPD_OK);
    CHECK(decoded.picoseconds == 295000 && decoded.has_minimum_cycles == 0
          && decoded.minimum_cycles == 0);
    timing = {5000, 8, 1};
    CHECK(spd_timing_set(image.data(), image.size(), SPD_TIMING_TRRD_L, timing) == SPD_OK);
    CHECK(spd_timing_get(image.data(), image.size(), SPD_TIMING_TRRD_L, &decoded) == SPD_OK);
    CHECK(decoded.picoseconds == 5000 && decoded.minimum_cycles == 8
          && decoded.has_minimum_cycles == 1);
    timing = {312, 0, 0};
    CHECK(spd_timing_set(image.data(), image.size(), SPD_TIMING_TCK_MIN, timing) == SPD_OK);
    CHECK(spd_timing_get(image.data(), image.size(), SPD_TIMING_TCK_MIN, &decoded) == SPD_OK);
    CHECK(decoded.picoseconds == 312 && decoded.has_minimum_cycles == 0);
    CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == typed_crc);
    CHECK(crc.stored != crc.calculated); // Typed edits also leave CRC untouched.
    CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated);
    const auto typed_snapshot = image;
    timing = {295001, 0, 0};
    CHECK(spd_timing_set(image.data(), image.size(), SPD_TIMING_TRFC1, timing)
          == SPD_VALUE_OUT_OF_RANGE);
    CHECK(image == typed_snapshot);
    timing = {5000, 8, 0}; // Absent cycles cannot carry a nonzero value.
    CHECK(spd_timing_set(image.data(), image.size(), SPD_TIMING_TRRD_L, timing)
          == SPD_INVALID_ARGUMENT);
    CHECK(image == typed_snapshot);
    CHECK(spd_timing_get(image.data(), image.size(), SPD_TIMING_TRFC4, &decoded)
          == SPD_UNSUPPORTED_FIELD);
    image[1] = 0x11;
    CHECK(spd_timing_get(image.data(), image.size(), SPD_TIMING_TCK_MIN, &decoded)
          == SPD_UNSUPPORTED_REVISION);
    image[1] = 0x10;
    CHECK(spd_module_crc(image.data(), image.size(), &crc) == SPD_UNSUPPORTED_FIELD);

    const std::array<uint8_t, 4> serial{0, 0x80, 0xff, 7};
    const uint8_t part[] = "SYNTHETIC-DDR5";
    spd_manufacturing manufacturing{};
    CHECK(spd_part_number_set(image.data(), image.size(), part, sizeof part - 1) == SPD_OK);
    CHECK(spd_serial_number_set(image.data(), image.size(), serial.data()) == SPD_OK);
    CHECK(spd_manufacturing_get(image.data(), image.size(), &manufacturing) == SPD_OK);
    CHECK(manufacturing.part_number_len == 30 && manufacturing.part_number[29] == ' ');
    CHECK(std::memcmp(manufacturing.part_number, part, sizeof part - 1) == 0);
    CHECK(std::memcmp(manufacturing.serial, serial.data(), serial.size()) == 0);
    CHECK(spd_base_crc(image.data(), image.size(), &crc) == SPD_OK);
    CHECK(crc.stored == crc.calculated); // Manufacturing lies outside base CRC.

    // DDR4's independent module CRC must also be repaired explicitly.
    {
        std::array<uint8_t, 512> ddr4{};
        ddr4[1] = 0x13;
        ddr4[2] = 0x0c;
        ddr4[3] = 0x03;
        timing = {13750, 0, 0};
        CHECK(spd_timing_set(ddr4.data(), ddr4.size(), SPD_TIMING_TAA, timing) == SPD_OK);
        CHECK(spd_timing_get(ddr4.data(), ddr4.size(), SPD_TIMING_TAA, &decoded) == SPD_OK);
        CHECK(decoded.picoseconds == 13750 && decoded.has_minimum_cycles == 0);
        const uint8_t ddr4_part[] = "SYNTHETIC-DDR4";
        CHECK(spd_part_number_set(ddr4.data(), ddr4.size(), ddr4_part,
                                  sizeof ddr4_part - 1) == SPD_OK);
        CHECK(spd_serial_number_set(ddr4.data(), ddr4.size(), serial.data()) == SPD_OK);
        CHECK(spd_manufacturing_get(ddr4.data(), ddr4.size(), &manufacturing) == SPD_OK);
        CHECK(manufacturing.part_number_len == 20 && manufacturing.part_number[19] == ' '
              && manufacturing.part_number[20] == 0);
        CHECK(std::memcmp(manufacturing.part_number, ddr4_part, sizeof ddr4_part - 1) == 0);
        CHECK(std::memcmp(manufacturing.serial, serial.data(), serial.size()) == 0);
        CHECK(spd_repair_module_crc(ddr4.data(), ddr4.size(), &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
        const auto module_crc = crc.stored;
        CHECK(spd_image_write(ddr4.data(), ddr4.size(), 128, &replacement, 1) == SPD_OK);
        CHECK(spd_module_crc(ddr4.data(), ddr4.size(), &crc) == SPD_OK);
        CHECK(crc.stored == module_crc && crc.stored != crc.calculated);
        CHECK(spd_repair_module_crc(ddr4.data(), ddr4.size(), &crc) == SPD_OK);
        CHECK(crc.stored == crc.calculated);
    }

    const auto snapshot = image;
    const auto max_offset = std::numeric_limits<size_t>::max();
    CHECK(spd_image_write(image.data(), image.size(), max_offset, &replacement, 1)
          == SPD_OUT_OF_BOUNDS);
    CHECK(image == snapshot);
    CHECK(spd_image_write(image.data(), image.size(), image.size(), nullptr, 0) == SPD_OK);
    CHECK(spd_image_read(image.data(), image.size(), image.size() + 1, nullptr, 0)
          == SPD_OUT_OF_BOUNDS);
    CHECK(spd_base_crc(image.data(), 511, &crc) == SPD_TRUNCATED);
    CHECK(spd_crc16_xmodem(nullptr, 1, &standalone) == SPD_INVALID_ARGUMENT);
    CHECK(spd_repair_base_crc(image.data(), image.size(), nullptr) == SPD_INVALID_ARGUMENT);
    CHECK(image == snapshot);
    image[2] = 0x7f;
    CHECK(spd_header(image.data(), image.size(), &header) == SPD_OK);
    CHECK(header.memory_type == 0x7f);
    crc = {0xabcd, 0x1234};
    const auto unsupported_snapshot = image;
    CHECK(spd_repair_base_crc(image.data(), image.size(), &crc) == SPD_UNSUPPORTED_MEMORY_TYPE);
    CHECK(image == unsupported_snapshot && crc.stored == 0xabcd && crc.calculated == 0x1234);
    image[2] = 0x12;

    SyntheticBackend backend;
    backend.bytes = image;
    const spd_io io{&backend, synthetic_cpp_read, synthetic_cpp_write};
    std::array<uint8_t, 4> output{};
    CHECK(spd_io_read(&io, 0, output.data(), output.size()) == SPD_OK);
    CHECK(backend.reads == 1 && std::memcmp(output.data(), image.data(), output.size()) == 0);
    const std::array<uint8_t, 2> input{0x32, 0x76};
    CHECK(spd_io_write(&io, 20, input.data(), input.size()) == SPD_OK);
    CHECK(backend.writes == 1 && backend.bytes[20] == input[0] && backend.bytes[21] == input[1]);
    CHECK(spd_io_read(&io, 0, nullptr, 0) == SPD_OK && backend.reads == 2);
    CHECK(spd_io_write(&io, 0, nullptr, 0) == SPD_OK && backend.writes == 2);
    auto calls = backend.reads;
    CHECK(spd_io_read(&io, max_offset, output.data(), 1) == SPD_OUT_OF_BOUNDS);
    CHECK(backend.reads == calls);
    const spd_io absent{nullptr, nullptr, nullptr};
    CHECK(spd_io_read(&absent, 0, output.data(), output.size()) == SPD_INVALID_ARGUMENT);
    CHECK(spd_io_write(nullptr, 0, input.data(), input.size()) == SPD_INVALID_ARGUMENT);

    backend.fail_read = true;
    output.fill(0xee);
    CHECK(spd_io_read(&io, 0, output.data(), output.size()) == SPD_CALLBACK_ERROR);
    CHECK(backend.reads == calls + 1 && output[0] == image[0] && output[1] == 0xee);
    backend.fail_write = true;
    calls = backend.writes;
    CHECK(spd_io_write(&io, 30, input.data(), input.size()) == SPD_CALLBACK_ERROR);
    CHECK(backend.writes == calls + 1 && backend.bytes[30] == input[0] && backend.bytes[31] == 0);

    std::puts("C++ C ABI: synthetic FPM/EDO/SDR/DDR1-5 fields, explicit integrity repair and callbacks passed");
    return 0;
}
