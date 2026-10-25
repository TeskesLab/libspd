/* SPDX-License-Identifier: Apache-2.0 */
/**
 * @file libspd.h
 * @brief Stable ABI 1 for inspecting and editing caller-owned SPD byte images.
 *
 * No image handle, allocation, hardware driver, or configuration validator is
 * provided. All offsets and lengths are in bytes; ranges below are half-open.
 * Typical workflow: obtain an image (optionally through spd_io_read()), inspect
 * spd_header()/spd_identity(), decode selected fields, edit a writable copy,
 * explicitly repair the applicable checksum/CRC, then persist it if desired.
 * Integrity checks return both values: compare stored == calculated yourself.
 * A mismatch still returns SPD_OK. Decoding does not require valid integrity.
 *
 * @section spd_memory Common ownership, memory, and concurrency contract
 * The caller owns all buffers, output objects, callback state, and I/O objects.
 * Nothing is retained after a call. A buffer pointer may be NULL only when its
 * associated length is zero (non-NULL empty buffers are also allowed). Every
 * buffer length must be <= PTRDIFF_MAX. Required scalar/struct outputs and io
 * pointers must be non-NULL, correctly aligned, live objects. Nonempty buffers
 * must be valid for their entire declared length within one allocation for the
 * entire call. Arbitrary dangling/invalid pointers cannot be detected safely.
 *
 * Mutable buffers and output objects must not overlap any input buffer, another
 * output, or the io object. This applies to whole declared buffers, not merely
 * selected byte ranges: image copies do not support overlapping buffers.
 * Callers provide synchronization: no concurrent access to memory being
 * modified, and callback state/backend access must be synchronized by the caller.
 * Independent calls using disjoint writable memory need no library-owned state.
 *
 * Unless a callback has run, errors leave image buffers and output objects
 * untouched. Image setters are atomic on error, preserve unrelated bytes, and
 * validate representation, not physical feasibility or cross-field consistency.
 * No operation implicitly repairs integrity. Explicit repair functions change
 * only their own checksum/CRC storage bytes; raw image writes may deliberately
 * replace any requested bytes, including integrity storage. Struct member
 * values are the ABI contract; native padding is not serialized SPD data.
 *
 * Normal paths use checked operations. An unexpected internal Rust panic aborts
 * rather than unwinding across C or becoming a status. Callbacks must not panic,
 * throw, or longjmp across the ABI. See docs/ffi.md for longer C/C++ examples,
 * docs/fields.md for modern field offsets, and docs/legacy.md for legacy fields.
 */
#ifndef LIBSPD_H
#define LIBSPD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** @brief Fixed-width signed 32-bit status; not an enum-sized ABI parameter. */
typedef int32_t spd_status;
/** @brief Status codes. Unsupported means unimplemented/unverified, not invalid hardware. */
enum {
    SPD_OK = 0, /**< Operation succeeded; integrity values may still differ. */
    SPD_INVALID_ARGUMENT = 1, /**< NULL required pointer, excessive length, unknown timing ID, or malformed native timing value. */
    SPD_OUT_OF_BOUNDS = 2, /**< Requested image range exceeds len, or offset + length overflows size_t. */
    SPD_TRUNCATED = 3, /**< Image lacks discriminator, header, field, or integrity-domain bytes needed by this operation. */
    SPD_UNSUPPORTED_MEMORY_TYPE = 4, /**< Memory-type byte is not supported for the requested interpretation. */
    SPD_CALLBACK_ERROR = 5, /**< Invoked callback returned nonzero; partial effects are possible. */
    SPD_UNSUPPORTED_REVISION = 6, /**< On-image revision is outside the operation's supported matrix. */
    SPD_UNSUPPORTED_MODULE_TYPE = 7, /**< Complete raw module-type byte is outside the supported typed layout. */
    SPD_UNSUPPORTED_FIELD = 8, /**< Known field or integrity domain is not available for this layout. */
    SPD_INVALID_ENCODING = 9, /**< Image/proposed encoding cannot yield the required decoded value (including invalid timing/text encodings). */
    SPD_VALUE_OUT_OF_RANGE = 10, /**< Requested value cannot be represented exactly; no rounding is performed. */
    SPD_UNSUPPORTED_TIMEBASE = 11 /**< Timing timebase encoding is not supported; no scale is guessed. */
};

/** @brief Literal first-four-byte view, not generation-aware identity. */
typedef struct spd_header_info {
    uint8_t size_raw; /**< Raw byte 0; no size/capacity interpretation. */
    uint8_t revision; /**< Raw byte 1; NOT the pre-DDR3 semantic SPD revision. */
    uint8_t memory_type; /**< Raw byte 2, including unknown codes. */
    uint8_t module_type_raw; /**< Raw byte 3; NOT pre-DDR3 semantic module identity. */
} spd_header_info;

/** @brief Native 16-bit stored/computed CRC values; compare for equality. */
typedef struct spd_crc_check {
    uint16_t stored; /**< Stored little-endian image bytes decoded to a native integer. */
    uint16_t calculated; /**< CRC-16/XMODEM computed over the selected domain. */
} spd_crc_check;

/** @brief Generation-aware raw identity slots, without typed-codec validation. */
typedef struct spd_identity_info {
    uint8_t memory_type; /**< FPM=0x01, EDO=0x02, SDR=0x04, DDR=0x07, DDR2=0x08, DDR3=0x0b, DDR4=0x0c, DDR5=0x12. */
    uint8_t revision; /**< Raw SPD revision: byte 62 before DDR3, otherwise byte 1. */
    uint8_t module_type; /**< Raw byte 20 for DDR2, byte 3 for DDR3+; zero when absent. */
    uint8_t has_module_type; /**< Exactly 0 (absent for FPM/EDO/SDR/DDR) or 1 (present, even if value is zero). */
} spd_identity_info;

/** @brief Native 8-bit stored/computed additive checksum values. */
typedef struct spd_checksum_check {
    uint8_t stored; /**< Raw storage byte 63. */
    uint8_t calculated; /**< Sum of bytes [0,63) modulo 256, NOT an additive complement. */
} spd_checksum_check;

/**
 * @brief Separate pre-DDR4 manufacturing result, preserving legacy widths.
 * Identifiers, location, dates, serials, and revisions are uninterpreted bytes:
 * no JEP106 names/parity, date validation, or serial endianness is inferred.
 */
typedef struct spd_legacy_manufacturing_info {
    uint8_t manufacturer[8]; /**< Eight raw bytes pre-DDR3; two DDR3 bytes followed by six zero output bytes. */
    uint8_t manufacturer_len; /**< Exactly 8 before DDR3 or 2 for DDR3; excludes zero output tail. */
    uint8_t location; /**< One raw manufacturing-location byte. */
    uint8_t date_raw[2]; /**< Two raw date bytes; format is generation/vendor dependent. */
    uint8_t serial[4]; /**< Four raw serial bytes in image order, not an integer. */
    uint8_t part_number[18]; /**< All 18 stored bytes, including padding/non-ASCII; not a NUL-terminated string. */
    uint8_t revision[2]; /**< Both stored revision bytes, unlike the modern one-byte revision. */
} spd_legacy_manufacturing_info;

/**
 * @brief Timing IDs passed as uint32_t, not enum-sized ABI values.
 * These name distinct fields, not a promise of support in every generation.
 * All decoded durations use picoseconds regardless of the stored unit.
 */
enum {
    SPD_TIMING_TCK_MIN = 1, /**< tCKmin: minimum clock-cycle period. */
    SPD_TIMING_TCK_MAX = 2, /**< tCKmax: maximum clock-cycle period. */
    SPD_TIMING_TAA = 3, /**< tAA: minimum CAS access latency in time, not a CAS-latency mask. */
    SPD_TIMING_TRCD = 4, /**< tRCD: minimum row activation to column read/write delay. */
    SPD_TIMING_TRP = 5, /**< tRP: minimum row precharge time. */
    SPD_TIMING_TRAS = 6, /**< tRAS: minimum active-to-precharge time. */
    SPD_TIMING_TRC = 7, /**< tRC: minimum row-cycle (activate-to-activate, same bank) time. */
    SPD_TIMING_TWR = 8, /**< tWR: minimum write-recovery time. */
    SPD_TIMING_TRFC1 = 9, /**< tRFC1: refresh recovery/cycle time for the 1x refresh mode. */
    SPD_TIMING_TRFC2 = 10, /**< tRFC2: refresh recovery/cycle time for the 2x refresh mode. */
    SPD_TIMING_TRFC4 = 11, /**< tRFC4: refresh recovery/cycle time for the 4x refresh mode. */
    SPD_TIMING_TRFC_SB = 12, /**< tRFCsb: same-bank refresh recovery/cycle time. */
    SPD_TIMING_TRRD_S = 13, /**< tRRD_S: short activate-to-activate delay across different bank groups. */
    SPD_TIMING_TRRD_L = 14, /**< tRRD_L: long activate-to-activate delay within the same bank group. */
    SPD_TIMING_TCCD_L = 15, /**< tCCD_L: long column-to-column command delay within a bank group. */
    SPD_TIMING_TCCD_L_WR = 16, /**< tCCD_L_WR: long write-to-write column-command delay. */
    SPD_TIMING_TCCD_L_WR2 = 17, /**< tCCD_L_WR2: separately encoded DDR5 write-to-write timing constraint; not an alias of tCCD_L_WR. */
    SPD_TIMING_TFAW = 18, /**< tFAW: minimum four-activate window. */
    SPD_TIMING_TWTR_S = 19, /**< tWTR_S: short internal write-to-read delay across different bank groups. */
    SPD_TIMING_TWTR_L = 20, /**< tWTR_L: long internal write-to-read delay within the same bank group. */
    SPD_TIMING_TRTP = 21, /**< tRTP: minimum read-to-precharge delay. */
    SPD_TIMING_TRAC = 22, /**< tRAC: FPM/EDO row-address (RAS) access time, not DDR tAA. */
    SPD_TIMING_TCAC = 23  /**< tCAC: FPM/EDO column-address (CAS) access time, not DDR tAA. */
};

/** @brief Exact duration plus optional independently stored lower clock limit. */
typedef struct spd_timing {
    uint32_t picoseconds; /**< Duration in integer ps; not a frequency or cycle count. */
    uint32_t minimum_cycles; /**< Lower nCK limit, 0..255 when present; must be 0 when absent. */
    uint32_t has_minimum_cycles; /**< Exactly 0 or 1. Present zero nCK is distinct from absence. */
} spd_timing;

/**
 * @brief DDR4/DDR5 manufacturing data, separate from legacy manufacturing.
 * Numeric byte arrays remain raw: no manufacturer/date/serial decoding or text
 * validation is performed by the getter. Native struct padding is not image data.
 */
typedef struct spd_manufacturing {
    uint8_t module_manufacturer[2]; /**< Two raw module-manufacturer identifier bytes. */
    uint8_t location; /**< One raw manufacturing-location byte. */
    uint8_t date_raw[2]; /**< Two raw date bytes; no BCD/calendar validation. */
    uint8_t serial[4]; /**< Four raw serial bytes in image order. */
    uint8_t part_number[30]; /**< Raw fixed-width part number including stored spaces; DDR4 unused ten-byte output tail is zero. Not a C string. */
    uint8_t part_number_len; /**< Stored field width: 20 for DDR4, 30 for DDR5, not trimmed text length. */
    uint8_t module_revision; /**< One raw module-revision byte. */
    uint8_t dram_manufacturer[2]; /**< Two raw DRAM-manufacturer identifier bytes. */
    uint8_t dram_stepping; /**< One raw DRAM-stepping byte. */
} spd_manufacturing;

/** @brief Return the native ABI version. @return Always 1 for this ABI. */
uint32_t spd_abi_version(void);
/**
 * @brief Copy the literal first four bytes, preserving unknown memory types.
 * @param[in] data Image buffer; at least four bytes are required for success.
 * @param[in] len Accessible image length, not a declared SPD capacity.
 * @param[out] out Required raw-header result. Use spd_identity() for semantic slots.
 * @return SPD_OK, SPD_INVALID_ARGUMENT, or SPD_TRUNCATED; no codec validation.
 */
spd_status spd_header(const uint8_t *data, size_t len, spd_header_info *out);
/**
 * @brief Read generation-aware raw identity without validating codec support.
 * @param[in] data Image; success needs 63 bytes before DDR3, four for DDR3+.
 * @param[in] len Accessible image length.
 * @param[out] out Required identity result; values do not certify revision/module support.
 * @return SPD_OK or an argument, truncation, or unsupported-memory-type error.
 * Unknown memory codes are rejected here, unlike spd_header().
 */
spd_status spd_identity(const uint8_t *data, size_t len, spd_identity_info *out);
/**
 * @brief Check the pre-DDR3 byte sum, not a CRC or balancing complement.
 * @param[in] data Image of a supported FPM/EDO/SDR/DDR/DDR2 revision; needs 64 bytes.
 * @param[in] len Accessible image length.
 * @param[out] out Required stored/calculated pair: sum [0,63) modulo 256 versus byte 63.
 * @return SPD_OK even on mismatch; DDR3+ returns SPD_UNSUPPORTED_FIELD.
 * Revision validation uses the legacy matrix described below.
 */
spd_status spd_checksum(const uint8_t *data, size_t len, spd_checksum_check *out);
/**
 * @brief Explicitly repair only checksum byte 63; same applicability as spd_checksum().
 * @param[in,out] data Writable image, at least 64 bytes on success.
 * @param[in] len Accessible image length.
 * @param[out] out Required post-repair pair with stored == calculated.
 * @return SPD_OK or the corresponding checksum/argument error; errors change nothing.
 */
spd_status spd_repair_checksum(uint8_t *data, size_t len, spd_checksum_check *out);
/**
 * @brief Copy a raw image range without parsing or integrity validation.
 * @param[in] data Source image, disjoint from the entire output buffer.
 * @param[in] len Accessible image length; may be zero.
 * @param[in] offset Byte offset; offset + output_len must not overflow or exceed len.
 * @param[out] output Destination for exactly output_len bytes.
 * @param[in] output_len Copy length; zero still requires offset <= len.
 * @return SPD_OK, SPD_INVALID_ARGUMENT, or SPD_OUT_OF_BOUNDS; no partial copy on error.
 */
spd_status spd_image_read(const uint8_t *data, size_t len, size_t offset,
                          uint8_t *output, size_t output_len);
/**
 * @brief Replace a raw image range atomically on error; no integrity repair.
 * @param[in,out] data Destination image, disjoint from the entire input buffer.
 * @param[in] len Accessible image length; may be zero.
 * @param[in] offset Byte offset; offset + input_len must not overflow or exceed len.
 * @param[in] input Source for exactly input_len bytes.
 * @param[in] input_len Copy length; zero still requires offset <= len.
 * @return SPD_OK, SPD_INVALID_ARGUMENT, or SPD_OUT_OF_BOUNDS.
 */
spd_status spd_image_write(uint8_t *data, size_t len, size_t offset,
                           const uint8_t *input, size_t input_len);
/**
 * @brief Check the generation-specific base CRC; mismatch is successful data.
 * DDR3 needs 128 bytes and revision 0x10/0x11: byte 0 bit 7 set covers [0,117),
 * clear covers [0,126); storage is little-endian [126,128). DDR4 covers [0,126)
 * with storage [126,128), requiring 128 bytes. DDR5 covers [0,510) with storage
 * [510,512), requiring 512 bytes (base block only). DDR4/DDR5 base CRC does not
 * impose the typed revision/module matrix. Pre-DDR3 returns SPD_UNSUPPORTED_FIELD.
 * @param[in] data Image buffer.
 * @param[in] len Accessible image length.
 * @param[out] out Required stored/calculated CRC pair.
 * @return SPD_OK even on mismatch, or an argument/format/truncation error.
 */
spd_status spd_base_crc(const uint8_t *data, size_t len, spd_crc_check *out);
/**
 * @brief Repair only the two base-CRC storage bytes identified by spd_base_crc().
 * Coverage selection (including DDR3 byte 0 bit 7) is preserved.
 * @param[in,out] data Writable image; same lengths and applicability as spd_base_crc().
 * @param[in] len Accessible image length.
 * @param[out] out Required post-repair pair with stored == calculated.
 * @return SPD_OK or the corresponding base-CRC/argument error; errors change nothing.
 */
spd_status spd_repair_base_crc(uint8_t *data, size_t len, spd_crc_check *out);
/**
 * @brief Compute standalone CRC-16/XMODEM over arbitrary bytes, not an SPD domain.
 * Polynomial 0x1021, initial value 0, no reflection, no final XOR.
 * @param[in] data Input buffer, optionally NULL for empty input.
 * @param[in] len Number of bytes; zero is valid and produces CRC zero.
 * @param[out] out Required native 16-bit result.
 * @return SPD_OK or SPD_INVALID_ARGUMENT.
 */
spd_status spd_crc16_xmodem(const uint8_t *data, size_t len, uint16_t *out);

/**
 * @section spd_typed Selected typed-field support
 * This is not a complete JEDEC decoder. Supported revisions: FPM/EDO/SDR
 * 0x00/0x01, DDR 0x10, DDR2 0x12 (revision at byte 62); DDR3 0x10/0x11
 * (byte 1). No module-type gate is imposed for these common legacy fields.
 * DDR4 requires revision 0x11/0x13 and complete module byte 0x03; DDR5 requires
 * revision 0x10 and complete module byte 0x02. Unknown revisions/module bytes
 * are rejected rather than assumed compatible.
 *
 * Timing availability: FPM/EDO tRAC/tCAC; SDR tCKmin/tRP/tRCD/tRAS;
 * DDR/DDR2 tRP/tRCD/tRAS; DDR3 tCKmin/tAA/tWR/tRCD/tRP/tRAS/tRC/tRFC1/tRTP/tFAW;
 * DDR4 tCKmin/tCKmax/tAA/tRCD/tRP/tRRD_S/tRRD_L/tCCD_L/tRFC1/tRFC2/tRFC4;
 * DDR5 all IDs except tRFC4/tRRD_S/tRAC/tCAC.
 *
 * DDR4 timing byte 17 must be 0 (MTB 125 ps, FTB 1 ps). DDR3 validates a
 * supported rational timebase, with fine offsets used only for revision 0x11.
 * No value is rounded, no missing cycle count is inferred, and no semantic
 * consistency or operating-speed validation occurs. Unknown timing IDs return
 * SPD_INVALID_ARGUMENT; known unavailable IDs return SPD_UNSUPPORTED_FIELD.
 */
/**
 * @brief Decode one supported timing into exact integer picoseconds.
 * @param[in] data Image containing identification and the selected timing/timebase bytes.
 * @param[in] len Accessible image length; no full image is required. Legacy
 * pre-DDR3 needs byte 62 even for earlier timing fields; other field extents vary.
 * @param[in] field One of the 23 SPD_TIMING_* numeric IDs.
 * @param[out] out Required timing; absent cycles are returned as flag 0/count 0.
 * @return SPD_OK or an argument, unsupported, truncated, or invalid-encoding error.
 * DDR5 lower-nCK fields return flag 1 even for stored zero cycles.
 */
spd_status spd_timing_get(const uint8_t *data, size_t len, uint32_t field,
                          spd_timing *out);
/**
 * @brief Encode one timing exactly, preserving unrelated bits/bytes; atomic on error.
 * @param[in,out] data Writable image with the same required field extent as the getter.
 * @param[in] len Accessible image length.
 * @param[in] field One of the SPD_TIMING_* IDs.
 * @param[in] value Exact desired timing. Flag must be 0/1; flag 0 requires count
 * 0, flag 1 requires count <=255. All legacy/DDR4 and plain DDR5 fields require
 * absent cycles; DDR5 tRRD_L/tCCD_L/tCCD_L_WR/tCCD_L_WR2/tFAW/tWTR_L/tWTR_S/tRTP
 * require present cycles, including zero. Duration is always in ps.
 * @return SPD_OK; malformed native flag/count gives SPD_INVALID_ARGUMENT;
 * well-formed but nonrepresentable timing gives SPD_VALUE_OUT_OF_RANGE.
 * Format/timebase/truncation errors also apply. No integrity repair is performed.
 */
spd_status spd_timing_set(uint8_t *data, size_t len, uint32_t field,
                          spd_timing value);
/**
 * @brief Copy complete raw DDR4/DDR5 manufacturing data for a supported typed layout.
 * @param[in] data Image; success requires 353 bytes for DDR4 or 555 for DDR5.
 * @param[in] len Accessible image length.
 * @param[out] out Required modern result, with explicit part-number width and zero unused tail.
 * @return SPD_OK or argument/unsupported/truncation error. Pre-DDR4 uses
 * spd_legacy_manufacturing() instead and returns SPD_UNSUPPORTED_FIELD here.
 */
spd_status spd_manufacturing_get(const uint8_t *data, size_t len,
                                 spd_manufacturing *out);
/**
 * @brief Copy complete raw pre-DDR4 manufacturing data without losing legacy widths.
 * @param[in] data Supported legacy image; needs 99 bytes before DDR3 or 148 for DDR3.
 * @param[in] len Accessible image length.
 * @param[out] out Required legacy result; eight/two manufacturer bytes, 18 part
 * bytes, and two revision bytes. Unused manufacturer output tail is zero.
 * @return SPD_OK or argument/unsupported/truncation error; DDR4+ gives SPD_UNSUPPORTED_FIELD.
 */
spd_status spd_legacy_manufacturing(const uint8_t *data, size_t len,
                                    spd_legacy_manufacturing_info *out);
/**
 * @brief Set printable ASCII part number, right-padding the stored field with spaces.
 * @param[in,out] data Supported typed image; success needs 91 bytes pre-DDR3,
 * 146 for DDR3, 349 for DDR4, or 551 for DDR5, plus revision identification.
 * @param[in] len Accessible image length.
 * @param[in] part_number Exactly part_number_len bytes, each 0x20..0x7e; no implicit strlen.
 * @param[in] part_number_len At most 18 before DDR4, 20 DDR4, 30 DDR5.
 * Zero permits NULL and clears the whole field to spaces. A terminating NUL is
 * not accepted as text and should not be included in the length.
 * @return SPD_OK or argument/format/truncation/encoding/range error. Errors are
 * atomic; no checksum/CRC is repaired.
 */
spd_status spd_part_number_set(uint8_t *data, size_t len,
                              const uint8_t *part_number, size_t part_number_len);
/**
 * @brief Replace four raw serial bytes without numeric-endian interpretation.
 * @param[in,out] data Supported typed image; success needs 99 bytes pre-DDR3,
 * 126 DDR3, 329 DDR4, or 521 DDR5, plus revision identification.
 * @param[in] len Accessible image length.
 * @param[in] serial Required pointer to exactly four readable, disjoint bytes;
 * not optional even if len is zero.
 * @return SPD_OK or argument/unsupported/truncation error; atomic, no integrity repair.
 */
spd_status spd_serial_number_set(uint8_t *data, size_t len, const uint8_t *serial);
/**
 * @brief Check the separate module CRC for the supported DDR4 typed layouts.
 * Covers [128,254); little-endian storage [254,256). Needs 256 bytes and the
 * typed revision/module gate. Other supported generations have no such domain.
 * @param[in] data Image buffer.
 * @param[in] len Accessible image length.
 * @param[out] out Required stored/calculated pair.
 * @return SPD_OK even on mismatch, or argument/unsupported/truncation error.
 */
spd_status spd_module_crc(const uint8_t *data, size_t len, spd_crc_check *out);
/**
 * @brief Repair only bytes 254/255, not the base CRC or manufacturing data.
 * @param[in,out] data Writable image; same applicability/256-byte minimum as spd_module_crc().
 * @param[in] len Accessible image length.
 * @param[out] out Required post-repair pair with stored == calculated.
 * @return SPD_OK or the corresponding module-CRC/argument error; errors change nothing.
 */
spd_status spd_repair_module_crc(uint8_t *data, size_t len, spd_crc_check *out);

/**
 * @section spd_callbacks Synchronous whole-buffer callback contract
 * A valid I/O call invokes its selected present callback exactly once, including
 * zero length; validation failure or a missing selected callback invokes none.
 * Return zero for success; every nonzero int32_t maps to SPD_CALLBACK_ERROR,
 * without preserving the backend code. Callbacks must enforce backend capacity:
 * the library checks offset + len overflow, but cannot know storage bounds.
 *
 * Buffers are borrowed for the callback duration only: honor their full length
 * and exclusivity, do not retain pointers, and do not unwind/throw/longjmp.
 * Context may be NULL. Buffer memory must not alias io/context storage that the
 * callback accesses incompatibly. The caller owns context lifetime and backend
 * synchronization. A failed callback can leave partial output/hardware/storage
 * changes; no rollback is promised. There is no discovery, retry, splitting,
 * paging, timing, protection management, or bus access supplied by the library.
 */
/**
 * @brief Application-defined synchronous read under the callback contract.
 * @param[in,out] context Opaque caller state, possibly NULL; interpretation is application-defined.
 * @param[in] offset Backend byte offset.
 * @param[out] output Borrowed destination for len bytes; may be NULL for zero length.
 * @param[in] len Requested byte count (including zero).
 * @return Zero on success, any nonzero int32_t on failure; partial output is possible.
 */
typedef int32_t (*spd_read_callback)(void *context, size_t offset,
                                     uint8_t *output, size_t len);
/**
 * @brief Application-defined synchronous write under the callback contract.
 * @param[in,out] context Opaque caller state, possibly NULL; interpretation is application-defined.
 * @param[in] offset Backend byte offset.
 * @param[in] input Borrowed source for len bytes; may be NULL for zero length.
 * @param[in] len Requested byte count (including zero).
 * @return Zero on success, any nonzero int32_t on failure; partial backend writes are possible.
 */
typedef int32_t (*spd_write_callback)(void *context, size_t offset,
                                      const uint8_t *input, size_t len);

/** @brief Caller-owned callback table; either direction may be omitted. */
typedef struct spd_io {
    void *context; /**< Opaque borrowed state passed unchanged to callbacks; may be NULL. */
    spd_read_callback read; /**< Read callback or NULL; NULL makes spd_io_read() invalid. */
    spd_write_callback write; /**< Write callback or NULL; NULL makes spd_io_write() invalid. */
} spd_io;

/**
 * @brief Invoke io->read once synchronously, including for an empty request.
 * @param[in] io Required live aligned callback table; selected callback must be present.
 * @param[in] offset Backend byte offset; offset + len must fit size_t.
 * @param[out] output Destination for len bytes, subject to common nonoverlap rules.
 * @param[in] len Requested byte count, <= PTRDIFF_MAX; zero permits NULL output.
 * @return SPD_OK for callback zero; SPD_CALLBACK_ERROR for nonzero (possibly
 * partially modified output); SPD_INVALID_ARGUMENT for invalid pointer/length
 * or missing callback; SPD_OUT_OF_BOUNDS for offset overflow. Pre-callback
 * errors leave output untouched. Backend capacity is the callback's responsibility.
 */
spd_status spd_io_read(const spd_io *io, size_t offset,
                       uint8_t *output, size_t len);
/**
 * @brief Invoke io->write once synchronously, including for an empty request.
 * @param[in] io Required live aligned callback table; selected callback must be present.
 * @param[in] offset Backend byte offset; offset + len must fit size_t.
 * @param[in] input Source for len bytes, subject to common nonoverlap rules.
 * @param[in] len Requested byte count, <= PTRDIFF_MAX; zero permits NULL input.
 * @return SPD_OK for callback zero; SPD_CALLBACK_ERROR for nonzero (possibly
 * partial backend writes); SPD_INVALID_ARGUMENT for invalid pointer/length or
 * missing callback; SPD_OUT_OF_BOUNDS for offset overflow. Pre-callback errors
 * invoke no backend operation. No rollback, retry, or integrity repair occurs.
 */
spd_status spd_io_write(const spd_io *io, size_t offset,
                        const uint8_t *input, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* LIBSPD_H */
