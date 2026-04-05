# Architecture

## Role

`cartouche` encodes and decodes HDMI InfoFrames — the auxiliary metadata packets
transmitted in the data island periods between active video lines of the HDMI signal. The
negotiated video configuration cannot be signaled to the sink without correct InfoFrames:
the AVI InfoFrame communicates color space and colorimetry; the HDR InfoFrames communicate
mastering metadata and dynamic tone mapping; the HDMI Forum VSI communicates ALLM, VRR, and
DSC state.

Encoding and decoding are equally in scope. Decoding supports diagnostic tooling, protocol
inspection, receiver-side implementations, and any context where an InfoFrame needs to be
read off the wire and interpreted. Absence of an obvious use case is not grounds for
exclusion.

`cartouche` is a pure encoding/decoding library. It has no I/O. It does not know how
InfoFrames are transmitted or received; that is the integration layer's concern. It does
not know whether a constructed InfoFrame is consistent with the negotiated configuration;
that is `cartouche-validate`'s concern.

---

## Scope

cartouche covers:

- all five HDMI 2.1 InfoFrame types: AVI, Audio, HDR Static Metadata, HDMI Forum
  Vendor-Specific, and Dynamic HDR,
- encoding every type to its wire packet representation,
- decoding every type from wire packet bytes,
- a unified `IntoPackets` encoding interface that works for both single-packet and
  multi-packet (Dynamic HDR) frame types,
- structured decode warnings (checksum mismatch, reserved fields, out-of-spec values)
  without discarding the decoded frame,
- checksum computation on encode and verification on decode,
- an `InfoFrame` top-level enum covering all known types and an `Unknown` catch-all.

The following are out of scope:

- **Validation against a negotiated configuration** — a separate `cartouche-validate`
  crate checks consistency between an InfoFrame and a `NegotiatedConfig`. This requires
  a `concordance` dependency that does not belong in a pure encoding library.
- **Transmission and reception** — cartouche produces and consumes bytes. The integration
  layer is responsible for writing those bytes into data island periods and reading them
  back.
- **CEC** — a separate protocol crate.
- **SCDC** — `culvert`.

---

## Dependencies

```
display-types  ──►  cartouche
```

`cartouche` depends only on `display-types` for shared HDMI vocabulary types
(`HdmiForumFrl`, color format enums, and similar). It does not depend on `concordance`,
`culvert`, `plumbob`, or any hardware abstraction crate.

`cartouche` is `#![no_std]` and does not require an allocator.

---

## The Wire Format

### Traditional InfoFrames

Every traditional InfoFrame (AVI, Audio, HDR Static Metadata, HDMI Forum VSI) is
transmitted as a single 31-byte packet:

```
Byte 0:    Header — Type Code   (1 byte)
Byte 1:    Header — Version     (1 byte)
Byte 2:    Header — Length      (1 byte, payload byte count, not including header or checksum)
Byte 3:    Checksum             (1 byte, computed such that sum of all 31 bytes = 0x00 mod 256)
Bytes 4–30: Payload             (up to 27 bytes)
```

The checksum covers all 31 bytes including itself. On encode it is computed from the
header and payload and written into byte 3. On decode it is verified; a mismatch is a
warning, not an error.

### Dynamic HDR InfoFrames

Dynamic HDR (HDMI 2.1 §10.2.8) breaks the 31-byte model. The metadata payload — which
may be in formats including HDR10+ (ETSI TS 103 433) and SL-HDR — can reach several
hundred bytes and is transmitted across a sequence of 29-byte-payload packets. Each
packet in the sequence carries a subset of the metadata; the receiver reassembles them in
order. The packet header structure is the same as traditional InfoFrames; the payload
contains sequence and format fields in addition to the metadata chunk.

This distinction shapes the core encoding abstraction: a design that assumes every
InfoFrame is 31 bytes cannot accommodate Dynamic HDR without a breaking change.

---

## The Two-Level Abstraction

### Typed structs (logical layer)

Each InfoFrame type is a typed Rust struct representing the full logical content of the
frame. Fields are named and typed; no raw bytes appear at this layer. Constructing a
frame means filling in the struct. Reading a decoded frame means reading its fields.

### `IntoPackets` (wire layer)

All InfoFrame types implement an `IntoPackets` trait that yields an iterator of
`[u8; 31]` wire packets:

```rust
pub trait IntoPackets {
    type Iter: Iterator<Item = [u8; 31]>;
    fn into_packets(self) -> Self::Iter;
}
```

For traditional InfoFrame types, the iterator yields exactly one item. For Dynamic HDR,
it yields as many 31-byte packets as the metadata requires. The integration layer's
transmission loop is the same regardless of frame type:

```rust
for packet in frame.into_packets() {
    transmit(&packet);
}
```

No allocation is required for encoding. The iterator is a state machine over the typed
struct.

### Decode

Decode is the inverse of encoding, at the same two levels.

For single-packet frame types, decode takes a `[u8; 31]` and returns a typed struct
plus any warnings:

```rust
impl AviInfoFrame {
    pub fn decode(packet: &[u8; 31]) -> Decoded<AviInfoFrame, AviWarning, DecodeError>;
}
```

For Dynamic HDR, decode takes a slice of packets (`&[[u8; 31]]`) that the caller has
already gathered into a sequence. cartouche assembles the payload and parses it;
accumulating packets from the wire is the caller's responsibility. No allocation is
required in cartouche.

The top-level decode entry point takes a single `[u8; 31]` and dispatches on the type
code:

```rust
pub fn decode(packet: &[u8; 31]) -> Decoded<InfoFrame, Warning, DecodeError>;
```

For Dynamic HDR packets, this returns a partial result with a continuation — the caller
feeds subsequent packets until the sequence is complete. The exact API for stateful
multi-packet decode is defined in the Dynamic HDR section below.

---

## The `InfoFrame` Enum

The top-level type for decode dispatch and for callers iterating a set of frames to
transmit:

```rust
#[non_exhaustive]
pub enum InfoFrame {
    Avi(AviInfoFrame),
    Audio(AudioInfoFrame),
    HdrStatic(HdrStaticInfoFrame),
    HdmiForumVsi(HdmiForumVsi),
    DynamicHdr(DynamicHdrInfoFrame),
    Unknown { type_code: u8, version: u8, payload: [u8; 27] },
}
```

`Unknown` carries the raw bytes of the payload unmodified. Nothing is discarded. The
`payload` field is 27 bytes (maximum traditional InfoFrame payload); unknown type codes
that arrive in multi-packet form are handled separately.

`InfoFrame` implements `IntoPackets`. Dispatching over the enum gives a uniform encoding
path for callers that hold a collection of frames.

---

## Decode Error Handling

### Checksum

The checksum byte is verified on decode. If the sum of all 31 bytes is not 0x00 mod 256,
a `Warning::ChecksumMismatch { expected: u8, found: u8 }` is attached to the decoded
result. The frame is still returned; the caller decides whether to act on it. This follows
the piaf and concordance pattern: a suspicious but parseable input is a warning, not a
parse failure.

On encode, the checksum is computed from scratch. The input struct carries no checksum
field; the byte is always derived, never round-tripped from user data.

### Unknown type codes

A packet whose type code does not correspond to a known InfoFrame type decodes to
`InfoFrame::Unknown`. The type code, version, and raw payload bytes are preserved. The
checksum is still verified and a warning attached if it fails.

### Out-of-spec field values

Fields that contain values outside the specified range (a reserved colorimetry code,
an undefined EOTF value, a VIC not listed in the spec) decode to their typed
representation where possible and attach a warning. Where no typed representation exists
for the value (e.g. a reserved enum discriminant), a typed `Warning` variant carries the
raw byte. The frame is still returned.

### Truncated input

A packet that is shorter than the length declared in its header cannot be recovered from.
This is the one case that returns a hard `DecodeError::Truncated`. All other anomalies
are warnings.

---

## InfoFrame Types

### AVI InfoFrame

The most information-dense InfoFrame and the most important for correct display. Carries
the color space, colorimetry, quantization range, aspect ratio, active format, VIC, and
pixel repetition count. A correctly configured AVI InfoFrame is required for the sink to
display the signal correctly.

The AVI InfoFrame is defined in CEA-861. The HDMI 2.1 spec extends it with the Additional
Colorimetry Extension (ACE) field for wide-color-gamut formats (DCI-P3, BT.2100, and
others) that the original colorimetry field cannot express.

Notable complexity:

- The extended colorimetry field is only valid when the primary colorimetry field is
  set to `Extended`; decode must reconstruct the intended colorimetry from both fields
  together.
- The ACE field is only valid when the extended colorimetry field indicates it; the
  full colorimetry path spans three fields.
- The bar data fields (top/bottom/left/right bar pixel counts) are present only when
  their respective "bar data present" flags are set; unused fields contain undefined bytes
  that must not be interpreted.
- The RGB quantization range field applies only when the color space is RGB; YCC
  quantization has its own field.

### Audio InfoFrame

Carries channel count, coding type, sample frequency, sample size, channel allocation,
LFE playback level, and downmix inhibit. Required for the sink to configure its audio
decoder and speaker mapping correctly.

The coding type field has a `ReferToStream` value that defers to the audio stream's own
header; this is the common case for compressed formats. The sample frequency and size
fields similarly have `ReferToStream` variants.

### HDR Static Metadata InfoFrame

Carries the EOTF (Electro-Optical Transfer Function) and the HDR static metadata as
defined in CTA-861-G. The static metadata includes display mastering luminance, primaries,
white point, and content light level (MaxCLL / MaxFALL). Required for HDR10 and HLG
content.

The metadata type field selects among several static metadata descriptor types defined
by CTA-861. Type 1 (SMPTE ST 2086 mastering display metadata + MaxCLL/MaxFALL) is the
common case for HDR10.

### HDMI Forum Vendor-Specific InfoFrame

Carries HDMI Forum–defined auxiliary signaling: ALLM (Auto Low Latency Mode), VRR
(Variable Refresh Rate), DSC (Display Stream Compression), QMS (Quick Media Switching),
and FRL rate signaling. Defined by the HDMI Forum VSDB and SCDS blocks.

This is the InfoFrame most tightly coupled to HDMI 2.1 features. Several of its fields
mirror the state written to SCDC registers by culvert and plumbob.

### Dynamic HDR InfoFrame

Carries per-frame or per-scene dynamic tone mapping metadata for formats including HDR10+
(ETSI TS 103 433) and SL-HDR. Unlike all other InfoFrame types, the payload is variable
length and spans multiple 29-byte-payload packets transmitted in sequence.

The Dynamic HDR InfoFrame introduces additional packet-level structure: each packet
carries a sequence number, a byte count, and a format identifier in addition to the
metadata chunk. The logical frame is reassembled by concatenating the metadata chunks
across all packets.

#### Encoding

Encoding a `DynamicHdrInfoFrame` produces a sequence of `[u8; 31]` packets via
`IntoPackets`. The iterator handles packet boundary alignment, sequence numbering, and the
final partial-chunk packet automatically. No allocation required.

#### Decoding

Decoding requires a full sequence of packets. The caller is responsible for collecting the
packets (the wire packet's sequence field indicates position; the byte count field indicates
when the sequence is complete). Once the full sequence is available, it is passed to
`DynamicHdrInfoFrame::decode_sequence(&[[u8; 31]])`, which assembles and parses the
payload. No allocation is required in cartouche; the caller provides the buffer.

The metadata format identifier selects the interpretation of the payload bytes. Unknown
format identifiers decode to `DynamicHdrInfoFrame::Unknown { format_id: u8, payload: ...
}` with the raw payload preserved.

---

## `no_std` Compatibility

`cartouche` declares `#![no_std]` and `#![forbid(unsafe_code)]`. The full API is
available without an allocator. No `Vec`, no heap. All encoding is done through
iterators over stack-allocated state; all decoding takes caller-provided slices.

An `alloc` feature and a `std` feature (which implies `alloc`) are reserved for future
use if a higher-level convenience API (e.g., collecting all packets from a frame into a
`Vec<[u8; 31]>`) is added. The core encode/decode API is always alloc-free.

---

## Design Principles

- **Complete coverage.** All five InfoFrame types are implemented. Every field specified
  in the standard is represented. No field is omitted because it seems niche or unlikely
  to be needed. What is relevant to the caller is the caller's decision, not cartouche's.
- **Typed fields, not raw bytes.** Every InfoFrame field is a named, typed Rust value.
  Color spaces are enums, not integers. VICs are validated values, not raw `u8`s. Raw
  bytes appear only in `Unknown` variants, where they are preserved exactly because the
  type is not understood.
- **Warnings without data loss.** Anomalous input (bad checksum, reserved field,
  out-of-spec value) produces a warning on the returned frame, not an error. The caller
  receives the data and the warning; nothing is silently discarded. Truncation is the
  only hard error.
- **Checksum is a wire detail.** The checksum byte is computed from the rest of the
  frame on encode and verified on decode. It is not a field in the typed struct; callers
  do not set or read it. It is always correct on encode; a mismatch on decode is reported
  as a warning.
- **Dynamic HDR is first-class.** The `IntoPackets` abstraction and the decode API are
  designed for both single-packet and multi-packet frames from the start. Adding Dynamic
  HDR does not require changes to the interface any other InfoFrame type uses.
- **No I/O.** cartouche produces and consumes bytes. When and how those bytes move across
  a wire is never cartouche's concern.
- **No allocation.** All encoding and decoding is done without a heap. The integration
  layer may choose to collect packets into a `Vec`; cartouche does not need to.
- **No unsafe code.** `#![forbid(unsafe_code)]`.
- **Stable consumer types.** All public structs are `#[non_exhaustive]` for forward
  compatibility.

---

## Implementation Plan

### Phase 0 — Project infrastructure

Before any InfoFrame logic:

- `Cargo.toml`: crate metadata (`name`, `version`, `edition`, `rust-version`,
  `description`, `repository`, `license`, `readme`, `keywords`, `categories`),
  `[dependencies]` (`display-types`), feature flags (`alloc`, `std`).
- `#![no_std]`, `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]` in `lib.rs`.
- `LICENSE` (MPL-2.0).
- `README.md` with badges (CI, crates.io, docs.rs, license, rustc), crate role, usage
  example, stack position diagram, feature table, documentation links.
- `CHANGELOG.md` in Keep a Changelog format.
- `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md`, matching the sibling crates.
- `.github/workflows/ci.yml`: fmt check, clippy (`-D warnings`), docs
  (`-D missing_docs`), test, no_std build check, alloc-only build check.
- `.github/workflows/audit.yml`: `rustsec/audit-check` on Cargo.toml / Cargo.lock
  changes.
- `.github/workflows/publish.yml`: tag-triggered publish gated to commits reachable
  from `main`, running the full CI suite before `cargo publish`.
- Coverage ratchet in `ci.yml`: `cargo-llvm-cov` measurement, baseline check against
  `.coverage-baseline`, automatic `ci/coverage-ratchet` PR on improvement.
- `doc/` directory: `setup.md`, `testing.md`, `roadmap.md` (this file is
  `architecture.md`).
- `.coverage-baseline` file.

### Phase 1 — Core types and infrastructure (0.1.0 prerequisite)

The shared machinery that all InfoFrame types depend on:

- `IntoPackets` trait: iterator-based encoding interface, yields `[u8; 31]`.
- Checksum computation: `compute_checksum(header_and_payload: &[u8]) -> u8`, used by
  all encode paths.
- Checksum verification: called on every decode path, attaches `Warning::ChecksumMismatch`
  on mismatch.
- `DecodeError` type: `Truncated` is the only hard decode failure.
- `Warning` type (or per-frame warning enums): `ChecksumMismatch`, `ReservedFieldNonZero`,
  `UnknownEnumValue { field: &'static str, raw: u8 }`.
- `Decoded<T, W>` type: pairs a decoded frame with its warnings.
- `InfoFrame` top-level enum with all five variants and `Unknown`.
- `InfoFrame` implement `IntoPackets` (dispatches to variant impls).
- Top-level `decode(packet: &[u8; 31]) -> Decoded<InfoFrame, Warning, DecodeError>`.

### Phase 2 — Traditional InfoFrame types (0.1.0)

Implement encode and decode for each single-packet InfoFrame type. Each type gets:

- a typed struct with named fields,
- `IntoPackets` impl that builds the 31-byte packet, computes the checksum,
- `decode(&[u8; 31]) -> Decoded<Self, Warning, DecodeError>`,
- a variant in `InfoFrame`,
- rustdoc on every public item,
- unit tests covering round-trip encode/decode, out-of-spec field warnings, and
  checksum mismatch handling.

Order of implementation (roughly increasing complexity):

1. `AudioInfoFrame` — straightforward field mapping.
2. `HdrStaticInfoFrame` — EOTF, metadata type, static metadata fields.
3. `HdmiForumVsi` — ALLM, VRR, DSC, FRL fields; more fields but regular structure.
4. `AviInfoFrame` — largest, most complex, highest-priority for correctness. Extended
   colorimetry and ACE field chain, bar data conditionals, RGB vs. YCC quantization
   range handling.

### Phase 3 — Dynamic HDR InfoFrame (0.1.0 or 0.2.0)

- `DynamicHdrInfoFrame` typed struct, wrapping per-format variants.
- `IntoPackets` impl: packet boundary alignment, sequence numbering, per-packet byte
  count and format identifier fields, final partial-chunk handling.
- `decode_sequence(&[[u8; 31]]) -> Decoded<DynamicHdrInfoFrame, Warning, DecodeError>`:
  assembles payload from the packet sequence, dispatches on format identifier.
- `DynamicHdrInfoFrame` variant in `InfoFrame`.
- Stateful decode context for callers that receive packets one at a time and need to
  determine when a sequence is complete.
- HDR10+ (ETSI TS 103 433) format: full metadata struct.
- SL-HDR format: full metadata struct.
- `Unknown { format_id: u8, payload: ... }` catch-all for unrecognised format identifiers.

### Phase 4 — Documentation and examples

- `doc/testing.md`: testing strategy, round-trip property testing approach, how to write
  tests against the simulated decode path.
- Simulation example (`examples/roundtrip` or similar): construct one of each InfoFrame
  type, encode to packets, decode from packets, assert field equality.
- `doc/roadmap.md`: what is released, what is planned.
- Pre-publish docs review: verify every public item has a rustdoc comment, all links
  resolve, README matches the actual API.
