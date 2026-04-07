# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
