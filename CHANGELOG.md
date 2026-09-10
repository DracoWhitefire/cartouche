# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-05-23

### Changed

- **`display-types` updated to 0.4** — tracks DisplayID 2.x support added in `piaf` 0.4.1.

### Fixed

- `DynamicHdrInfoFrame::decode_sequence` — replaced two `match ... { Ok(v) => v, Err(e) => return Err(e) }`
  blocks with `?`, fixing a `clippy::question_mark` failure under current toolchain lints.
- `Arr127` (internal `serde` deserialization helper in `slhdr.rs`) — dropped its unused length
  field; the count was already sourced from the sibling `count` field on `SlHdrTable127`, so
  this was genuine dead code, not just an unused-but-needed value.

## [0.2.0] - 2026-04-11

### Added

- **HDR10+ decode and encode** — `Hdr10PlusMetadata` struct covering all ETSI TS 103 433-1
  fields: application identifier, distribution function, window descriptors, Bezier curve
  anchors, and tone mapping parameters. `DynamicHdrInfoFrame::decode_sequence` now returns
  `DynamicHdrInfoFrame::Hdr10Plus(Hdr10PlusMetadata)` for format identifier `0x04`.
  `IntoPackets` encodes a full `Hdr10PlusMetadata` back to the wire packet sequence.
- **SL-HDR decode and encode** — `SlHdrMetadata` struct covering ETSI TS 103 433-2 fields:
  mode, payload type, body fields (`SlHdrBody`), and lookup tables (`SlHdrTable127`).
  `DynamicHdrInfoFrame::decode_sequence` now returns `DynamicHdrInfoFrame::SlHdr(SlHdrMetadata)`
  for format identifier `0x02`. `IntoPackets` encodes a full `SlHdrMetadata` back to the
  wire packet sequence.
- **`IntoPackets` for `DynamicHdrInfoFrame`** — dynamic HDR frames can now be encoded to
  a packet sequence, completing the encode–decode symmetry for all InfoFrame types.
- **`serde` feature** — opt-in `Serialize` and `Deserialize` implementations on all public
  types. Warning enums derive `Serialize` only (their `field: &'static str` field makes
  `Deserialize` derivation unsound). `SlHdrTable127` uses a hand-written implementation
  to handle its `[u16; 127]` array correctly under all serde backends.
- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/cartouche`.

### Changed

- **`DynamicHdrInfoFrame` dispatch** — `decode_sequence` previously returned
  `DynamicHdrInfoFrame::Unknown { format_id: 0x04 }` for HDR10+ and
  `DynamicHdrInfoFrame::Unknown { format_id: 0x02 }` for SL-HDR. It now returns the
  typed `Hdr10Plus` and `SlHdr` variants respectively. Code that matched `Unknown` for
  these format identifiers must be updated. All other format identifiers continue to
  return `Unknown`.

## [0.1.0] - 2026-04-07

### Added

- **AVI InfoFrame** — full encode and decode covering all fields: color space,
  colorimetry (including Extended Colorimetry and ACE chain), quantization range,
  aspect ratio, active format, VIC, pixel repetition, and bar data fields.
- **Audio InfoFrame** — full encode and decode: channel count, coding type, sample
  frequency, sample size, channel allocation, LFE playback level, downmix inhibit.
- **HDR Static Metadata InfoFrame** — full encode and decode: EOTF, metadata type,
  display mastering luminance, primaries, white point, MaxCLL, MaxFALL.
- **HDMI Forum VSI** — full encode and decode: ALLM, VRR, DSC, QMS, FRL rate, and
  all auxiliary signaling fields defined by the HDMI Forum VSDB.
- **Dynamic HDR InfoFrame (partial)** — `DynamicHdrFragment::decode` decodes individual
  Dynamic HDR packets exposing `seq_num`, `total_bytes`, `format_id`, and `chunk` fields.
  `DynamicHdrInfoFrame::decode_sequence` assembles a complete packet sequence and returns
  `DynamicHdrInfoFrame::Unknown { format_id }` for all format identifiers. Per-format
  metadata structs (HDR10+, SL-HDR) and `IntoPackets` are planned for a future release.
- **`IntoPackets` trait** — encoding interface returning `Decoded<Iter, Warning>`,
  pairing the packet iterator with any encode-time warnings; works for both
  single-packet and multi-packet frame types.
- **`InfoFrame` enum** — encode-path top-level enum covering all five InfoFrame types
  and an `Unknown` catch-all; implements `IntoPackets`.
- **`InfoFramePacket` enum** — decode-path top-level type returned by the top-level
  `decode` dispatch.
- **`Decoded<T, W>`** — pairs a decoded frame with its warnings; warning storage is
  feature-gated between `Vec<W>` (`alloc`/`std`) and `[Option<W>; 8]` (bare `no_std`).
- **Per-frame warning enums** — `AviWarning`, `AudioWarning`, `HdrStaticWarning`,
  `HdmiForumVsiWarning`, `DynamicHdrWarning`, each with `ChecksumMismatch`,
  `ReservedFieldNonZero`, and `UnknownEnumValue` variants.
- **Checksum** — computed on encode, verified on decode; mismatch is a warning, not
  an error.
- **`DecodeError`** — `Truncated { claimed }` is the only hard decode error.
- **`no_std` + `alloc` + `std` support** — all three build tiers are explicitly
  supported and verified in CI.
- **`serde` feature** — derives `Serialize`/`Deserialize` on all public types.
- **Fuzz targets** — one target per InfoFrame type in `fuzz/fuzz_targets/`, exercising
  the no-panic and round-trip invariants via `cargo-fuzz`.
- **Round-trip example** — `examples/roundtrip` constructs, encodes, decodes, and
  asserts field equality for each InfoFrame type.
