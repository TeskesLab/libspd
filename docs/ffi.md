# C, C++, and foreign-language consumers

The public contract is [`include/libspd.h`](../include/libspd.h). This guide uses
ABI version 1; `spd_abi_version()` returns `uint32_t`. Include the matching header
and library, rather than recreating prototypes from an example. libspd is a
selected-field image codec, **not a DIMM driver or complete configuration
validator**. See the source-backed [DDR4/DDR5 field matrix](fields.md) and
[legacy field/revision matrix](legacy.md) before choosing a typed operation.
Recognition of a memory-type byte does not mean every revision, module type,
or field for that generation is supported. Unknown raw codes are not evidence
of a valid module.

## Include, build, and link

Use `#include "libspd.h"` and add `include/` to the compiler's include path.
Build the native artifacts with `cargo build -p libspd-ffi --release`.
On Linux, this produces `target/release/libspd.a` and `target/release/libspd.so`.
For a C99 source saved as `example.c`, typical commands from the repository root
are:

```sh
# Static: put the archive after the consumer object/source.
cc -std=c99 -Wall -Wextra -Werror -pedantic -Iinclude example.c \
  target/release/libspd.a -ldl -lpthread -lm -o example-static
# Shared: arrange the loader path explicitly for this local demonstration.
cc -std=c99 -Wall -Wextra -Werror -pedantic -Iinclude example.c \
  -Ltarget/release -lspd -o example-shared
LD_LIBRARY_PATH="$PWD/target/release" ./example-shared
```

These are Linux commands, not a claim of verified linking on other targets.
Use your deployment's normal loader policy, and consult [build instructions](build.md)
for requirements, C++ shared linking, target-dependent system libraries, and
recorded verification limits. The FFI crate archive does not bundle the
workspace-level public header.

## Ownership and execution model

- You own every image, input buffer, output object, callback context, and `spd_io`.
  There are no library-owned handles, allocation-returning APIs, or libspd free
  functions. Keep storage alive for the entire synchronous call.
- Header/identity, timing, metadata, raw image operations, and integrity operations
  work on the bytes passed to them. They never implicitly call an I/O callback,
  open a bus, discover a module, or write hardware. To edit a device snapshot,
  explicitly acquire bytes, edit the snapshot, choose integrity repair, validate
  externally, and separately decide whether/how to program it.
- A buffer pointer may be NULL **only when its length is zero**. Every length
  must be at most `PTRDIFF_MAX`. Required scalar/struct output pointers and the
  `spd_io` pointer are never optional. The serial setter requires four readable
  bytes; it has no length argument.
- Nonempty buffers must be valid, live, single-allocation regions. Objects must
  have their native alignment. Null checks cannot detect dangling pointers,
  undersized allocations, or incorrect bindings.
- Mutable buffers and outputs must not overlap input buffers, other outputs,
  or the `spd_io` object. This applies to the **entire image**, not just a selected
  copy range: `spd_image_read`/`spd_image_write` are not overlapping-copy APIs.
  Do not concurrently access memory being modified. Synchronization is yours.
- Validation/decoding/range failures leave image bytes and output objects
  unchanged. Callback failures are different: partial effects are possible.
  Successful setters preserve unrelated bytes, but never repair integrity bytes
  automatically. Representability is not cross-field or module-validity checking.

## Complete C99 snapshot example

Save this block alone as a C source file and link as above. It deliberately
constructs a **synthetic DDR5 image with a supported typed header**, not a valid
module specification, vendor dump, speed bin, or hardware-programming recipe.
Only fields used here are initialized meaningfully. The example changes a
representable duration while preserving its separate nCK constraint; this does
not establish that the resulting timing configuration is physically sound.

```c
#include "libspd.h"
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

static int checked(spd_status status, const char *operation)
{
    if (status == SPD_OK) return 1;
    fprintf(stderr, "%s failed: status %" PRId32 "\n", operation, status);
    return 0;
}

int main(void)
{
    uint8_t image[555] = {0};
    const uint8_t part[] = "SYNTHETIC-NOT-A-MODULE";
    spd_identity_info identity;
    spd_timing timing;
    spd_manufacturing metadata;
    spd_crc_check crc;
    size_t i;

    if (spd_abi_version() != UINT32_C(1)) {
        fputs("This example requires ABI 1\n", stderr);
        return 1;
    }
    image[1] = 0x10; /* Supported DDR5 revision. */
    image[2] = 0x12; /* DDR5. */
    image[3] = 0x02; /* Supported complete raw UDIMM type. */
    image[70] = 0x88; image[71] = 0x13; /* tRRD_L = 5000 ps, LE. */
    image[72] = 4; /* Separate lower-clock constraint, not inferred. */
    memset(image + 521, ' ', 30);
    memcpy(image + 521, part, sizeof part - 1);

    if (!checked(spd_identity(image, sizeof image, &identity), "identity"))
        return 1;
    printf("type=0x%02x revision=0x%02x module-present=%u module=0x%02x\n",
           (unsigned)identity.memory_type, (unsigned)identity.revision,
           (unsigned)identity.has_module_type, (unsigned)identity.module_type);

    if (!checked(spd_base_crc(image, sizeof image, &crc), "initial CRC"))
        return 1;
    printf("initial stored=%04x calculated=%04x (%s)\n",
           (unsigned)crc.stored, (unsigned)crc.calculated,
           crc.stored == crc.calculated ? "match" : "mismatch");

    if (!checked(spd_timing_get(image, sizeof image, SPD_TIMING_TRRD_L,
                                &timing), "get tRRD_L"))
        return 1;
    printf("tRRD_L=%" PRIu32 " ps, has-nCK=%" PRIu32 ", nCK=%" PRIu32 "\n",
           timing.picoseconds, timing.has_minimum_cycles, timing.minimum_cycles);
    timing.picoseconds = 6000; /* Preserve both cycle fields from the getter. */
    if (!checked(spd_timing_set(image, sizeof image, SPD_TIMING_TRRD_L,
                                timing), "set tRRD_L"))
        return 1;

    if (!checked(spd_manufacturing_get(image, sizeof image, &metadata),
                 "manufacturing"))
        return 1;
    /* Fixed-width bytes: never pass this array to %s or strlen. Escape
       nonprintable bytes; this is display policy, not metadata validation. */
    fputs("part=[", stdout);
    for (i = 0; i < metadata.part_number_len; ++i) {
        unsigned byte = metadata.part_number[i];
        if (byte >= 0x20 && byte <= 0x7e && byte != '\\')
            putchar((int)byte);
        else
            printf("\\x%02x", byte);
    }
    printf("] raw manufacturer=%02x:%02x raw date=%02x:%02x\n",
           (unsigned)metadata.module_manufacturer[0],
           (unsigned)metadata.module_manufacturer[1],
           (unsigned)metadata.date_raw[0], (unsigned)metadata.date_raw[1]);

    /* Explicit policy for this synthetic image: update the base CRC after
       editing. In an inspection tool, preserve a bad original for diagnosis. */
    if (!checked(spd_repair_base_crc(image, sizeof image, &crc), "repair CRC"))
        return 1;
    if (!checked(spd_base_crc(image, sizeof image, &crc), "verify CRC"))
        return 1;
    if (crc.stored != crc.calculated) {
        fputs("CRC mismatch after repair\n", stderr);
        return 1;
    }
    puts("Synthetic snapshot edited; no device was accessed.");
    return 0;
}
```

For plain timings, `has_minimum_cycles = 0` requires `minimum_cycles = 0`.
For DDR5 fields with nCK storage, presence is required, including when the
encoded count is zero. Preserve both members when changing only picoseconds.
The presence flag accepts only 0 or 1; present counts must fit `uint8_t`.
Setters reject non-exact units/ranges rather than rounding. See the matrices
for which fields support which representations.

## Identity, manufacturing, and integrity by generation

`spd_header` returns the first four raw slots, even for unknown memory types.
Its `revision` is always byte 1 and `module_type_raw` always byte 3: those are
**not** semantic legacy revision/module identity. Use `spd_identity` for the
generation-aware raw revision and optional module type; check
`has_module_type` rather than interpreting zero as absence. Identity is not
full typed-layout validation.

| Generation | Metadata getter / result | Integrity choice |
|---|---|---|
| FPM, EDO, SDRAM, DDR, DDR2 | `spd_legacy_manufacturing` / `spd_legacy_manufacturing_info`; 8 manufacturer bytes, 18 part bytes, 2 revision bytes; requires bytes through 98 | `spd_checksum`, `spd_repair_checksum` |
| DDR3 | Same legacy getter/result; 2 manufacturer bytes (`manufacturer_len = 2`, unused tail zero), 18 part bytes, 2 revision bytes; requires bytes through 147 | `spd_base_crc`, `spd_repair_base_crc` |
| DDR4 | `spd_manufacturing_get` / `spd_manufacturing`; 20 part bytes, requires 353 bytes | Base CRC and independent `spd_module_crc` / `spd_repair_module_crc` |
| DDR5 | Same modern getter/result; 30 part bytes, requires 555 bytes | Base CRC only; module CRC is unsupported |

These choices still require the supported typed revision/layout where applicable;
they do not widen the [field](fields.md) or [legacy](legacy.md) matrices.
Legacy metadata on DDR4/DDR5 and modern metadata on earlier generations return
`SPD_UNSUPPORTED_FIELD`. Modern unused part-array bytes are zeroed; they are not
part of the field. All part fields are fixed-width raw bytes, possibly spaces,
NULs, or non-ASCII, **not C strings**.

Manufacturer IDs, dates, locations, revision fields, and serial arrays remain
raw. No JEP106 name/bank/parity decoding, universal date/BCD validation, or serial
integer-endianness interpretation is supplied. The shared
`spd_part_number_set` and `spd_serial_number_set` dispatch by generation.
Part input is explicitly sized printable ASCII (0x20–0x7e), at most 18/20/30
bytes, and right-padded with spaces; zero length clears to spaces.

Checks return `SPD_OK` when calculation succeeds even if `stored != calculated`.
A mismatch is **data, not a status error**. Choose whether to preserve, report,
or explicitly repair it. The pre-DDR3 checksum is the low byte of the sum of
bytes 0–62, stored at 63, not an additive complement. CRC uses CRC-16/XMODEM;
DDR3 coverage depends on byte 0 bit 7, DDR4 base/module domains are separate,
and DDR5 base coverage ends at byte 509 (storage 510–511), not at the end of
all metadata. `spd_crc16_xmodem` calculates a supplied byte span without selecting
an SPD domain. Repair functions update only their stored integrity bytes.

## Application-owned I/O callbacks

`spd_io_read`/`spd_io_write` forward one whole-buffer operation synchronously.
A present callback is invoked exactly once, even for zero length. Return zero
for success; **any nonzero `int32_t` becomes `SPD_CALLBACK_ERROR`**, not that
callback's numeric code. Keep backend-specific diagnostics in your context if
needed. Either function pointer may be NULL; requesting a missing direction is
`SPD_INVALID_ARGUMENT`. A NULL context is permitted when the callback needs none.

The library checks pointer/length arguments and offset-plus-length overflow,
but cannot know backend capacity. Your callback must enforce bounds and perform
any real paging, transport, timing, protection, and synchronization. There are
no automatic retries, splitting, discovery, or bus calls. The supplied buffer
must not be retained past the callback. Do not alias it with context/backend
storage accessed incompatibly. Never throw, panic, or `longjmp` across the ABI.
An unexpected internal Rust panic aborts; it is not converted into a status.

A callback can fail after changing part of its output or backend; no rollback
is promised. Treat failed read output as potentially partial, and a failed write
as potentially applied in part. Reconcile actual backend state before deciding
what to do; blind retry is not a universal recovery strategy.

This second complete C99 program implements a real **memory-backed read**, not
a placeholder hardware adapter. Its bounds failures occur before copying, but
that stronger property belongs to this callback, not every possible backend.
The source array, context, `spd_io`, and destination are disjoint and live.

```c
#include "libspd.h"
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

struct memory_source {
    const uint8_t *bytes;
    size_t length;
};

static int32_t memory_read(void *context, size_t offset,
                           uint8_t *output, size_t length)
{
    const struct memory_source *source = context;
    if (source == NULL) return -1;
    if (offset > source->length || length > source->length - offset) return -2;
    if (length != 0) {
        if (source->bytes == NULL || output == NULL) return -3;
        memcpy(output, source->bytes + offset, length);
    }
    return 0;
}

int main(void)
{
    const uint8_t stored[] = {0x11, 0x22, 0x33, 0x44};
    struct memory_source source = {stored, sizeof stored};
    spd_io io = {&source, memory_read, NULL};
    uint8_t snapshot[2] = {0};
    spd_status status;

    if (spd_abi_version() != UINT32_C(1)) {
        fputs("This example requires ABI 1\n", stderr);
        return 1;
    }
    status = spd_io_read(&io, 1, snapshot, sizeof snapshot);
    if (status != SPD_OK) {
        fprintf(stderr, "read failed: %" PRId32 "\n", status);
        return 1;
    }
    if (snapshot[0] != 0x22 || snapshot[1] != 0x33) {
        fputs("Unexpected memory snapshot\n", stderr);
        return 1;
    }
    /* Backend capacity is checked by memory_read, not by spd_io_read. */
    status = spd_io_read(&io, sizeof stored, snapshot, sizeof snapshot);
    if (status != SPD_CALLBACK_ERROR) {
        fprintf(stderr, "Expected callback error, got %" PRId32 "\n", status);
        return 1;
    }
    /* This callback also handles zero length without dereferencing NULL. */
    status = spd_io_read(&io, sizeof stored, NULL, 0);
    if (status != SPD_OK) {
        fprintf(stderr, "empty read failed: %" PRId32 "\n", status);
        return 1;
    }
    puts("Memory-backed reads completed; no hardware was accessed.");
    return 0;
}
```

## Status handling

Always check `spd_status` before consuming an output. The ABI reports the error
class, not the Rust error's byte offset or detailed payload. Do not infer a
particular diagnostic precedence when several inputs are invalid.

| Status | Meaning and useful response |
|---|---|
| `SPD_OK` (0) | Operation completed; separately compare integrity values if applicable. |
| `SPD_INVALID_ARGUMENT` (1) | Null required pointer, invalid buffer length, missing callback, unknown numeric timing ID, or malformed native timing presence/count. Fix the call/binding; arbitrary pointer validity is still your responsibility. |
| `SPD_OUT_OF_BOUNDS` (2) | Raw image range exceeds the supplied buffer, or I/O offset-plus-length overflows. Correct range arithmetic; it does not identify backend capacity. |
| `SPD_TRUNCATED` (3) | The operation needs image bytes not supplied. Acquire the required section if available, or report a short snapshot; do not invent bytes for a real image. |
| `SPD_UNSUPPORTED_MEMORY_TYPE` (4) | The requested semantic operation cannot handle the type. Retain raw inspection where possible; do not guess a codec. |
| `SPD_CALLBACK_ERROR` (5) | Callback returned nonzero. Consult backend diagnostics and account for possible partial effects. |
| `SPD_UNSUPPORTED_REVISION` (6) | Typed codec has no supported revision match. Preserve raw bytes and consult the support matrix. |
| `SPD_UNSUPPORTED_MODULE_TYPE` (7) | Typed layout does not support the complete raw module-type byte. Do not mask unknown bits to force acceptance. |
| `SPD_UNSUPPORTED_FIELD` (8) | A known field/domain is unavailable for this generation/layout. Select a documented applicable operation or report it unavailable. |
| `SPD_INVALID_ENCODING` (9) | Existing/proposed encoded bytes are malformed for the operation. Preserve evidence and diagnose; do not silently reinterpret them. |
| `SPD_VALUE_OUT_OF_RANGE` (10) | A well-formed requested value cannot be represented exactly in the field. Choose an intentional representable value, not silent rounding. |
| `SPD_UNSUPPORTED_TIMEBASE` (11) | The codec cannot use the encoded timebase. Do not substitute an assumed scale. |

A known `SPD_TIMING_*` constant unsupported by the image is different from an
unknown numeric field ID: the former gives `SPD_UNSUPPORTED_FIELD` after
applicable layout checks, the latter `SPD_INVALID_ARGUMENT`. A future unfamiliar
status should be treated as failure, not success. None of these classes implies
that unconditional retry is appropriate.

## C++ and LLM/binding checklist

C++ includes `libspd.h` directly: it already supplies `extern "C"`. No separate
C++ wrapper is needed. `std::array`/`std::vector` or application RAII objects can
own images and contexts; keep storage stable and alive through each call and
prevent concurrent mutation/reallocation. There is no library handle to wrap
or destroy. Use callbacks compatible with the declared C function-pointer ABI;
catch C++ exceptions inside them and return a nonzero status instead of allowing
exceptions to cross into Rust. Do not use exception handling as a promise to
undo partial backend effects.

When generating a binding or asking an LLM to generate a consumer:

1. Copy exact prototypes from the matching header; bind `size_t` at native
   pointer width, `uint32_t` as unsigned 32-bit, and `spd_status`/callback results
   as signed `int32_t`. Timing IDs are `uint32_t`, not a language enum ABI.
2. Use native C struct field order, alignment, padding, and by-value passing
   (`spd_timing_set` takes `spd_timing` by value). Do not pack structs or replace
   integer presence fields with a language-native boolean. `spd_io` contains
   actual function pointers with the target calling convention.
3. Model NULL/zero-length rules, `PTRDIFF_MAX`, required outputs, allocation
   lifetime, and full-buffer non-aliasing explicitly. Keep callback functions
   and context alive; prevent a managed callback exception crossing the boundary.
4. Preserve both optional-nCK members, including present zero. Do not drop the
   cycle constraint when editing picoseconds or invent one for a plain field.
5. Never treat part-number arrays as C strings. Honor the returned modern width
   or the fixed legacy width of 18, and retain raw date/ID/serial bytes.
6. Check every status before reading outputs; compare stored/calculated integrity
   separately. Do not add implicit repair, hardware access, retries, frees, or
   unsupported-generation fallbacks. Consult the linked matrices, not a guessed
   universal SPD layout.
