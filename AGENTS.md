# libspd project rules

## Purpose and scope

- Build an Apache-2.0 SPD parsing and manipulation library usable from Rust, C, and C++.
- Keep image logic independent of hardware. Do not add I²C/SMBus drivers, OS device access, USB/serial protocols, programmer implementations, discovery, page selection, write-protection control, or PMIC/sideband services.
- External applications implement the Rust `Read`/`Write` traits or C callbacks. They own addresses, flat-offset mapping, paging, transfer limits, timing, retries, synchronization, protection, and verification.
- Use `docs/plan.md` for scope and evidence gates. Do not present a raw header or CRC checker as a complete SPD decoder.

## Repository structure

- `src/`: Rust image and codec core; `#![no_std]`, allocation-free, `#![forbid(unsafe_code)]`.
- `ffi/`: thin native ABI over the core; the only place for necessary, documented pointer/callback `unsafe`.
- `include/libspd.h`: shared C99/C++17 header. C++ calls the C ABI; no second parser or wrapper hierarchy.
- `examples/`: runnable synthetic examples, not purported real-module fixtures.
- `tests/`: consumer-visible core invariants and malformed-input boundaries.
- `docs/`: architecture, ABI contracts, verification instructions, and development plan. Keep README short.

## Data and codec rules

- Borrow caller-owned memory. Avoid allocation, copying, host-endian casts, packed struct overlays, and compiler bitfield layouts.
- Use checked bounds arithmetic before access or mutation. Failed edits must not change bytes.
- Preserve unknown/reserved bytes, raw type codes, and unsupported layouts. Report missing or unsupported sections explicitly.
- Dispatch by generation, revision, and module type where the field layout requires it. Never guess offsets or share layouts without evidence.
- Give timing and voltage values explicit units. Use checked integer arithmetic; presentation rounding must not affect stored values.
- Keep parsing, CRC validation, semantic validation, and mutation separate. A bad CRC must not hide recoverable fields.
- Never repair CRCs on parse, raw write, serialization, or callback invocation. Repair is explicit and limited to verified domains.
- Typed setters must preserve unrelated bits and validate representability before mutation.

## ABI and external adapters

- Core Rust interfaces are the source of behavior; native functions must delegate rather than duplicate codec logic.
- Use `repr(C)` structs, fixed-width integer fields/statuses, `size_t` lengths/offsets, and prefixed exports. Do not expose Rust enums, slices, strings, references, panics, or allocator ownership over C.
- Keep header layouts, Rust ABI types, error mapping, ABI version, examples, and docs synchronized. Record incompatible ABI changes and increase the ABI version.
- Document pointer validity, buffer sizes, lifetimes, aliasing, and thread ownership. Null/length checks cannot make a dangling pointer safe.
- Callbacks are synchronous exact-buffer operations. They must not throw/unwind or retain buffer pointers. Failures may leave partial transfers; never imply rollback or automatic retries.
- Read-only adapters must work without a write callback. Do not add global device state or implicit hardware operations.

## Evidence, licensing, and tests

- Cite field/domain sources and applicable revisions when adding codecs. The supplied capability map is secondary evidence, not a complete JEDEC specification.
- Do not copy GPL implementation code or proprietary firmware/decompiled code into this Apache-2.0 project. Do not commit confidential specs, credentials, personal serial numbers, or unlicensed images.
- Label synthetic inputs. Real fixtures need provenance, redistribution rights, redaction policy, and independently checked expected values.
- Add deterministic behavioral tests for bounds/overflow, truncation, unknown values, checksum coverage, error atomicity, and unchanged-byte preservation. Do not test source text, copied constants, or incidental wording.
- Before delivery, run `sh scripts/check.sh`. It checks Rust quality/tests and runs Rust plus linked C/C++ examples. Test any additional changed path directly. Never claim hardware validation from an in-memory callback example.
- Do not add dependencies or abstractions without a demonstrated need. Keep unsafe reasoning local and reviewable.
- Update relevant docs for permanent changes. Never leave callable stubs, fake fallbacks, or speculative supported-field claims.

## Workflow and publication

- Do not commit, push, publish tickets, or open pull requests unless authorized. Local planning belongs in `docs/plan.md`; ticket publication requires explicit permission.
- Before an authorized commit, run the review agent and resolve critical/high findings.
- Every commit must use `git commit -S`; do not disable signing or configure keys.
