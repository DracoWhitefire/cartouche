# Dynamic HDR InfoFrame — Full Support Implementation Plan

## Current state

`DynamicHdrFragment::decode` is complete: it decodes a single wire packet into a fragment
carrying `seq_num`, `total_bytes`, `format_id`, and `chunk`. The top-level dispatch
(`cartouche::decode`) routes Dynamic HDR packets to this path.

`DynamicHdrInfoFrame::decode_sequence` is a stub: it reads `format_id` from the first
packet, verifies checksums, and unconditionally returns
`DynamicHdrInfoFrame::Unknown { format_id }`. The metadata bytes in the chunks are
discarded rather than parsed.

`IntoPackets` for `DynamicHdrInfoFrame` is not implemented. The `InfoFrame::DynamicHdr`
arm in `frame.rs` returns `InfoFrameIter(None)` and yields zero packets (marked "Phase 3"
in a comment). `InfoFrameIter` is currently a thin wrapper around
`Option<SinglePacketIter>` and cannot handle a multi-packet sequence at all.

---

## Phases

### Phase 1 — `Unknown` catch-all with payload preservation

**Goal**: change `DynamicHdrInfoFrame::Unknown` so that raw payload bytes are retained,
making the variant re-encodable.

**Changes in `src/dynamic_hdr.rs`**:

Replace:
```rust
Unknown {
    format_id: u8,
}
```
with:
```rust
Unknown {
    format_id: u8,
    /// Raw metadata bytes concatenated from all chunks in the sequence.
    ///
    /// Only present in `alloc`/`std` builds. In bare `no_std` builds this
    /// field is absent and the payload is not retained — mirror the pattern
    /// used by `Decoded<T, W>` warning storage.
    #[cfg(any(feature = "alloc", feature = "std"))]
    payload: alloc::vec::Vec<u8>,
}
```

Update `decode_sequence` to concatenate `chunk[..chunk_len]` from each packet into the
`payload` field (alloc builds only).

This is a **breaking change** to `DynamicHdrInfoFrame::Unknown`. The variant is
`#[non_exhaustive]` so existing exhaustive matches already require a wildcard arm, but
constructing `Unknown` by field will break. Release in the same version as the per-format
structs (Phase 2/3) to batch breaking changes.

**Test additions** (`src/dynamic_hdr.rs` `#[cfg(test)]`):
- `decode_sequence_unknown_payload_assembled` — two-packet sequence with known chunk data;
  assert the concatenated payload matches.
- `decode_sequence_unknown_payload_roundtrip` — encode `Unknown { format_id, payload }`
  via `IntoPackets` (Phase 4), decode back, assert fields equal.

---

### Phase 2 — HDR10+ struct and decode

**Goal**: parse format identifier `0x04` (HDR10+, ETSI TS 103 433-1) into a typed struct.

#### 2a — Bitstream reader utility

HDR10+ metadata is a dense bitstream; every field is packed with no byte alignment.
Add a private `BitReader<'_>` struct in `src/dynamic_hdr.rs` (or `src/bitreader.rs`):

```rust
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,   // next bit to read within data[byte_pos], MSB-first
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self { ... }
    fn read_u8(&mut self, bits: u8) -> Result<u8, DecodeError>  { ... }
    fn read_u16(&mut self, bits: u8) -> Result<u16, DecodeError> { ... }
    fn read_u32(&mut self, bits: u8) -> Result<u32, DecodeError> { ... }
    fn read_bool(&mut self) -> Result<bool, DecodeError>         { ... }
    fn remaining_bits(&self) -> usize                             { ... }
}
```

ETSI TS 103 433-1 §6.1 specifies all fields in MSB-first bit order. The reader returns
`DecodeError::Truncated` (or a new `DecodeError::MalformedPayload` — see design notes
below) if the payload runs short.

A symmetric `BitWriter` is needed for Phase 4 encoding.

#### 2b — `Hdr10PlusMetadata` struct

Add to `src/dynamic_hdr.rs` (or a new `src/hdr10plus.rs` re-exported from `dynamic_hdr`):

```rust
pub struct Hdr10PlusMetadata {
    pub application_identifier: u8,        // 8 bits
    pub application_mode: u8,              // 8 bits (scene=0, frame=1)
    pub scene_frame_switching_flag: bool,  // 1 bit (mode 1 only)
    pub targeted_system_display_maximum_luminance: u32, // 27 bits, cd/m²
    pub targeted_system_display_actual_peak_luminance_flag: bool, // 1 bit
    // if flag above is set:
    pub targeted_system_display_actual_peak_luminance: Option<ActualPeakLuminance>,
    pub windows: Hdr10PlusWindows,         // 1..=3 windows; see below
    pub mastering_display_actual_peak_luminance_flag: bool,
    pub mastering_display_actual_peak_luminance: Option<ActualPeakLuminance>,
    pub color_saturation_mapping_flag: bool,
    pub color_saturation_weight: Option<u8>, // 6 bits; present only if flag set
}

/// Up to three tone-mapping windows.
///
/// Stored as a fixed array with a count rather than a `Vec` to allow `no_std`
/// without alloc.
pub struct Hdr10PlusWindows {
    pub count: u8,                         // 1..=3
    pub windows: [Hdr10PlusWindow; 3],     // only `windows[..count]` is valid
}

pub struct Hdr10PlusWindow {
    pub upper_left_corner_x: u16,          // 16 bits
    pub upper_left_corner_y: u16,          // 16 bits
    pub lower_right_corner_x: u16,         // 16 bits
    pub lower_right_corner_y: u16,         // 16 bits
    pub center_of_ellipse_x: u16,          // 16 bits
    pub center_of_ellipse_y: u16,          // 16 bits
    pub rotation_angle: u8,                // 8 bits
    pub semimajor_axis_internal_ellipse: u16,  // 16 bits
    pub semimajor_axis_external_ellipse: u16,  // 16 bits
    pub semiminor_axis_external_ellipse: u16,  // 16 bits
    pub overlap_process_option: bool,      // 1 bit
    pub maxscl: [u32; 3],                  // 3 × 17 bits
    pub average_maxrgb: u32,               // 17 bits
    pub distribution_maxrgb: DistributionMaxrgb,
    pub fraction_bright_pixels: u16,       // 10 bits
    pub tone_mapping_flag: bool,           // 1 bit
    pub knee_point: Option<KneePoint>,     // present if tone_mapping_flag
    pub bezier_curve_anchors: BezierAnchors, // present if tone_mapping_flag
}

pub struct DistributionMaxrgb {
    pub count: u8,                         // up to 15
    pub percentages: [u8; 15],             // 7 bits each
    pub percentiles: [u32; 15],            // 17 bits each
}

pub struct KneePoint {
    pub x: u16,                            // 12 bits
    pub y: u16,                            // 12 bits
}

pub struct BezierAnchors {
    pub count: u8,                         // up to 9
    pub anchors: [u16; 9],                 // 10 bits each
}

pub struct ActualPeakLuminance {
    pub num_rows: u8,                      // 5 bits
    pub num_cols: u8,                      // 5 bits
    /// Row-major; only `entries[..num_rows][..num_cols]` is valid.
    /// Values are 4-bit unsigned, representing 0.0–1.0 in steps of 1/15.
    pub entries: [[u8; 25]; 25],
}
```

Add `DynamicHdrInfoFrame::Hdr10Plus(Hdr10PlusMetadata)` variant.

Update `decode_sequence` to dispatch on `format_id == 0x04` and call
`Hdr10PlusMetadata::decode(payload: &[u8])`.

**Test additions**:
- Unit tests for `BitReader` (exact bit counts, short-read error, MSB-first ordering).
- `hdr10plus_round_trip` — construct a `Hdr10PlusMetadata` with known field values,
  encode (Phase 4), decode, assert equality.
- `hdr10plus_single_window_no_peak_lum` — minimal payload, no optional fields set.
- `hdr10plus_three_windows_full` — all optional fields set, three windows.
- `hdr10plus_malformed_short_payload_is_error` — payload too short to hold mandatory
  fields; assert `DecodeError`.

---

### Phase 3 — SL-HDR struct and decode

**Goal**: parse format identifier `0x02` (SL-HDR, ETSI TS 101 547-3 §4) into a typed
struct.

SL-HDR carries a simpler payload than HDR10+. The structure is defined in ETSI
TS 101 547-3 §4.3 "Dynamic metadata SEI message syntax". Key fields:

```rust
pub struct SlHdrMetadata {
    pub payload_mode: u8,              // 3 bits
    pub hdr_pic_colour_space_id: u8,   // 8 bits (mode 0/1 only)
    pub hdr_master_display_colour_space_id: u8, // 8 bits (mode 0/1 only)
    pub hdr_master_monitor_max_luminance: u16, // 16 bits (mode 0/1 only)
    pub sdr_pic_colour_space_id: u8,   // 8 bits
    pub sdr_master_display_colour_space_id: u8,
    pub sdr_master_monitor_max_luminance: u16,
    // ... additional mode-dependent fields per ETSI TS 101 547-3
}
```

The exact field set depends on `payload_mode`; consult ETSI TS 101 547-3 §4.3 for the
authoritative layout before writing parsing code.

Add `DynamicHdrInfoFrame::SlHdr(SlHdrMetadata)` variant. Update `decode_sequence` to
dispatch on `format_id == 0x02`.

**Test additions** (same pattern as HDR10+):
- Round-trip test per payload mode.
- Short-payload error test.

---

### Phase 4 — `IntoPackets` for `DynamicHdrInfoFrame`

**Goal**: implement multi-packet encoding.

#### 4a — Payload serialization

Each variant serializes its metadata into a byte buffer. The `BitWriter` (introduced in
Phase 2a) handles bit-level packing.

For `no_std` without alloc: the maximum HDR10+ payload is bounded by the fixed-size
struct (the largest case — three windows, all optional fields, max distribution points,
max bezier anchors — is under 200 bytes). Compute the exact maximum at compile time and
use a `[u8; MAX_DYNAMIC_HDR_PAYLOAD]` stack buffer. The `Unknown` variant cannot be
encoded in bare `no_std` builds (no payload storage).

For alloc builds: serialize into a `Vec<u8>`.

#### 4b — `DynamicHdrIter`

```rust
pub struct DynamicHdrIter {
    format_id: u8,
    total_bytes: u16,
    offset: usize,
    seq_num: u8,
    payload: PayloadBuf,  // feature-gated: Vec<u8> or [u8; MAX]; see above
}
```

`Iterator::next` implementation:
1. If `offset >= total_bytes as usize`, return `None`.
2. Take `chunk_len = (total_bytes as usize - offset).min(23)` bytes from
   `payload[offset..]`.
3. Build the packet header: `[0x20, 0x01, 4 + chunk_len as u8]`.
4. Set `packet[4] = seq_num`, `packet[5..7] = total_bytes.to_le_bytes()`,
   `packet[7] = format_id`.
5. Copy chunk into `packet[8..8 + chunk_len]`.
6. Compute and insert checksum at `packet[3]`.
7. Advance `offset += chunk_len`, `seq_num += 1`.
8. Return `Some(packet)`.

`DynamicHdrInfoFrame` implements `IntoPackets`:
```rust
impl IntoPackets for DynamicHdrInfoFrame {
    type Iter = DynamicHdrIter;
    type Warning = DynamicHdrWarning;

    fn into_packets(self) -> Decoded<DynamicHdrIter, DynamicHdrWarning> { ... }
}
```

#### 4c — `InfoFrameIter` refactor

`InfoFrameIter` currently wraps `Option<SinglePacketIter>`. Change it to dispatch over
both single-packet and multi-packet iterators without allocation:

```rust
pub struct InfoFrameIter(InfoFrameIterInner);

enum InfoFrameIterInner {
    Single(Option<SinglePacketIter>),
    Dynamic(DynamicHdrIter),
}

impl Iterator for InfoFrameIter {
    type Item = [u8; 31];
    fn next(&mut self) -> Option<[u8; 31]> {
        match &mut self.0 {
            InfoFrameIterInner::Single(s) => s.as_mut()?.next(),
            InfoFrameIterInner::Dynamic(d) => d.next(),
        }
    }
}
```

Update the `InfoFrame::DynamicHdr` arm in `InfoFrame::into_packets` to use
`InfoFrameIterInner::Dynamic`.

Remove the `// Phase 3` placeholder comment.

**Test additions**:
- `dynamic_hdr_hdr10plus_encodes_correct_packet_count` — assert that a known
  `Hdr10PlusMetadata` produces `ceil(payload_len / 23)` packets.
- `dynamic_hdr_hdr10plus_seq_nums_sequential` — assert `seq_num` is 0, 1, 2, … across
  all emitted packets.
- `dynamic_hdr_hdr10plus_total_bytes_consistent` — assert `total_bytes` is identical
  across all packets and equals the actual payload length.
- `dynamic_hdr_hdr10plus_final_packet_partial_chunk` — payload length not a multiple of
  23; assert the final `chunk_len` is correct and trailing bytes are zero.
- `dynamic_hdr_hdr10plus_all_checksums_valid` — for every emitted packet, assert that
  the 31-byte sum is 0x00 mod 256.
- `info_frame_dynamic_hdr_iter_round_trip` — encode via `InfoFrame::DynamicHdr`,
  reassemble with `decode_sequence`, assert equality.

Update the existing `dynamic_hdr_variant_yields_no_packets` test (currently asserts zero
packets) — it will fail once Phase 4 lands. Replace it with a test asserting at least one
packet is produced for a non-empty HDR10+ frame.

---

### Phase 5 — Fuzz target updates

Update `fuzz/fuzz_targets/dynamic_hdr.rs`:
- Add a round-trip fuzz target: for valid-looking input (first byte `0x20`), attempt
  `decode_sequence` → encode via `IntoPackets` → `decode_sequence` again; assert the
  second decode equals the first.
- Extend the no-panic target to exercise `Hdr10PlusMetadata::decode` directly on
  arbitrary byte slices (not just whole-packet sequences).

---

## Design notes

### `DecodeError` for malformed format-specific payloads

The current `DecodeError::Truncated` fires when the packet header's `length` field is
out of range. Parsing a well-formed packet sequence whose assembled payload is too short
for the declared format is a different class of error. Options:

1. **Reuse `Truncated`** — `claimed` already conveys "this many bytes were expected";
   fire it when the bitstream runs short inside `Hdr10PlusMetadata::decode`. Simple, but
   semantically imprecise.
2. **Add `DecodeError::MalformedPayload { format_id: u8 }`** — clearly separates
   packet-level truncation from format-level parse failure. Preferred; `DecodeError` is
   `#[non_exhaustive]` so this is not a breaking change.

Recommendation: add `MalformedPayload` in Phase 2.

### `no_std` payload buffer size

The maximum HDR10+ payload (ETSI TS 103 433-1 §6.1, all fields populated) works out to:
- Fixed-width fields: ~50 bytes
- Three windows × ~45 bytes each: ~135 bytes
- Per-window `distribution_maxrgb` (15 entries × 3 bytes): 45 bytes
- Per-window bezier anchors (9 × 10 bits ≈ 12 bytes): 36 bytes
- Two `ActualPeakLuminance` tables (25 × 25 × 4 bits ÷ 8 = 157 bytes each): 314 bytes

Total worst case: ~580 bytes. SL-HDR is smaller. Define:

```rust
pub(crate) const MAX_DYNAMIC_HDR_PAYLOAD: usize = 600;
```

This is a stack allocation only in bare `no_std` builds; alloc builds use `Vec<u8>`.
600 bytes on the stack is acceptable for an embedded context.

### Serde

All new public structs (`Hdr10PlusMetadata`, `Hdr10PlusWindow`, `SlHdrMetadata`, etc.)
should derive `Serialize`/`Deserialize` under `#[cfg(feature = "serde")]`, matching
every other public type in the crate.

### Versioning

Phases 1, 2, and 3 all add variants to `DynamicHdrInfoFrame`. Phase 1 also changes an
existing variant's field set. Bundle these into a single minor-version bump (0.2.0) to
avoid multiple breaking releases. Phase 4 (`IntoPackets`) and Phase 5 (fuzz) are
additive and can ship in the same release.
