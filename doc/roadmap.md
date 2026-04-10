# Roadmap

## Shipped

### 0.1.0 — Initial release

Full encode and decode for the four traditional HDMI 2.1 InfoFrame types, plus partial
Dynamic HDR support.

- `IntoPackets` trait — iterator-based encoding interface, no allocation required
- `AviInfoFrame` — full encode and decode including extended colorimetry, ACE, and bar data
- `AudioInfoFrame` — full encode and decode
- `HdrStaticInfoFrame` — full encode and decode: EOTF, metadata type, mastering metadata,
  MaxCLL, MaxFALL
- `HdmiForumVsi` — full encode and decode: ALLM, VRR, DSC, QMS, FRL rate
- `DynamicHdrFragment` — decode of individual Dynamic HDR packets, including
  `seq_num`, `total_bytes`, `format_id`, and `chunk` fields
- `DynamicHdrInfoFrame::decode_sequence` — assembles a packet sequence and dispatches on
  `format_id`; parses HDR10+ (format `0x04`) and SL-HDR (format `0x02`) into typed structs
  in all build configurations; unrecognised identifiers produce `Unknown`
- `DynamicHdrFragment` decode via the top-level `cartouche::decode` dispatch
- `InfoFrame` enum — encode-path top-level type; implements `IntoPackets`
- `InfoFramePacket` enum — decode-path top-level type returned by `cartouche::decode`
- `Decoded<T, W>` — decoded frame paired with per-frame warnings
- Per-frame warning enums with `ChecksumMismatch`, `ReservedFieldNonZero`,
  `UnknownEnumValue` variants
- Checksum computed on encode, verified on decode
- `no_std` + `alloc` + `std` support at all three build tiers
- `serde` feature: `Serialize`/`Deserialize` on all public types
- Fuzz targets for all five InfoFrame types (no-panic and round-trip invariants)
- Round-trip example (`examples/roundtrip`)

## Planned

### Broader test corpus

Real-world captured InfoFrame packets from hardware, providing regression coverage and
confidence for future changes to field decoding.
