# Roadmap

## Shipped

### 0.1.0 — Initial release

Full encode and decode for all five HDMI 2.1 InfoFrame types.

- `IntoPackets` trait — iterator-based encoding interface, no allocation required
- `AviInfoFrame` — full encode and decode including extended colorimetry, ACE, and bar data
- `AudioInfoFrame` — full encode and decode
- `HdrStaticInfoFrame` — full encode and decode: EOTF, metadata type, mastering metadata,
  MaxCLL, MaxFALL
- `HdmiForumVsi` — full encode and decode: ALLM, VRR, DSC, QMS, FRL rate
- `InfoFrame` enum — encode-path top-level type; implements `IntoPackets`
- `InfoFramePacket` enum — decode-path top-level type returned by `cartouche::decode`
- `Decoded<T, W>` — decoded frame paired with per-frame warnings
- Per-frame warning enums with `ChecksumMismatch`, `ReservedFieldNonZero`,
  `UnknownEnumValue` variants
- Checksum computed on encode, verified on decode
- `no_std` + `alloc` + `std` support at all three build tiers
- `serde` feature: `Serialize`/`Deserialize` on all public types

## Planned

### Dynamic HDR InfoFrame

Full encode and decode for the variable-length Dynamic HDR InfoFrame (HDMI 2.1 §10.2.8):

- `DynamicHdrInfoFrame` typed struct with per-format variants
- `IntoPackets` impl: packet boundary alignment, sequence numbering, final partial-chunk
  handling
- `decode_sequence(&[[u8; 31]])` — assemble payload from packet sequence, dispatch on
  format identifier
- HDR10+ (ETSI TS 103 433) format: full metadata struct
- SL-HDR format: full metadata struct
- `Unknown { format_id, payload }` catch-all for unrecognised format identifiers
- `DynamicHdrFragment` decode via the top-level `cartouche::decode` dispatch

### Fuzz targets

One fuzz target per InfoFrame type exercising the no-panic and round-trip invariants.
Integrated with the fuzz CI workflow.

### Broader test corpus

Real-world captured InfoFrame packets from hardware, providing regression coverage and
confidence for future changes to field decoding.
