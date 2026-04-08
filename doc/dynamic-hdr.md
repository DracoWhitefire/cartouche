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

**Known stub behaviour to fix:**
- `decode_sequence(&[])` currently returns `Ok(Unknown { format_id: 0 })`. An empty
  slice is not a valid sequence; Phase 1 changes this to an error (see below).
- `decode_sequence` does not validate that `seq_num` values are sequential, that
  `total_bytes` is consistent across packets, or that `format_id` is consistent across
  packets. Phase 1 adds warnings for all three.

---

## Phases

### Phase 1 — `Unknown` catch-all with payload preservation

**Goal**: change `DynamicHdrInfoFrame::Unknown` so that raw payload bytes are retained,
making the variant re-encodable. Harden `decode_sequence` with integrity checks.

#### 1a — `Unknown` variant payload field

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

This is a **breaking change** to `DynamicHdrInfoFrame::Unknown`. The variant is
`#[non_exhaustive]` so existing exhaustive matches already require a wildcard arm, but
constructing `Unknown` by field will break. Release in the same version as the per-format
structs (Phase 2/3) to batch breaking changes.

#### 1b — `decode_sequence` loop refactor

The current implementation does not extract chunk data from packets. The loop must be
restructured to (a) validate the sequence and (b) accumulate chunk bytes into the payload.

**Rewrite `decode_sequence` as follows:**

1. **Empty-sequence guard**: if `packets.is_empty()`, return
   `Err(DecodeError::EmptySequence)`. Add `EmptySequence` to `src/error.rs` — it is
   `#[non_exhaustive]` so this is not a breaking change. Update the `error.rs` doc comment
   to describe the new variant.

2. **Read invariants from the first packet**: `format_id = packets[0][7]`,
   `total_bytes = u16::from_le_bytes([packets[0][5], packets[0][6]])`.

3. **Packet loop** (replacing the existing loop):

   ```rust
   for (i, packet) in packets.iter().enumerate() {
       let length = packet[2];
       if length > 27 {
           return Err(DecodeError::Truncated { claimed: length });
       }

       // Checksum.
       let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
       if sum != 0x00 {
           let expected = crate::checksum::compute_checksum(
               packet[..30].try_into().unwrap()
           );
           decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
               expected,
               found: packet[3],
           });
       }

       // Sequence integrity.
       let seq_num = packet[4];
       if seq_num != i as u8 {
           decoded.push_warning(DynamicHdrWarning::OutOfOrderPacket {
               index: i as u8,
               found: seq_num,
           });
       }
       let pkt_total = u16::from_le_bytes([packet[5], packet[6]]);
       if pkt_total != total_bytes {
           decoded.push_warning(DynamicHdrWarning::InconsistentTotalBytes {
               packet: i as u8,
               expected: total_bytes,
               found: pkt_total,
           });
       }
       let pkt_fmt = packet[7];
       if pkt_fmt != format_id {
           decoded.push_warning(DynamicHdrWarning::InconsistentFormatId {
               packet: i as u8,
               expected: format_id,
               found: pkt_fmt,
           });
       }

       // Chunk accumulation (alloc builds only).
       #[cfg(any(feature = "alloc", feature = "std"))]
       {
           let chunk_len = length.saturating_sub(4).min(23) as usize;
           payload.extend_from_slice(&packet[8..8 + chunk_len]);
       }
   }
   ```

   `chunk_len = length.saturating_sub(4).min(23)` — same formula used by
   `DynamicHdrFragment::decode`. `saturating_sub` guards against a `length` of 0–3
   that would otherwise underflow.

4. **Build the result**: dispatch on `format_id` (initially only `Unknown`; later Phase 2
   adds `0x04`, Phase 3 adds `0x02`).

#### 1c — New `DynamicHdrWarning` variants

Add to `src/warn.rs` in `DynamicHdrWarning`:

```rust
/// A packet's `seq_num` field did not equal its position in the slice.
///
/// `index` is the packet's zero-based position in the `packets` slice;
/// `found` is the `seq_num` value actually present in the packet.
OutOfOrderPacket {
    /// Zero-based position of the packet in the sequence.
    index: u8,
    /// The `seq_num` value found in the packet header.
    found: u8,
},
/// A packet's `total_bytes` field differs from the first packet's value.
///
/// `total_bytes` must be identical across all packets in a sequence.
InconsistentTotalBytes {
    /// Zero-based index of the inconsistent packet.
    packet: u8,
    /// The value declared in the first packet.
    expected: u16,
    /// The value found in this packet.
    found: u16,
},
/// A packet's `format_id` field differs from the first packet's value.
///
/// `format_id` must be identical across all packets in a sequence.
InconsistentFormatId {
    /// Zero-based index of the inconsistent packet.
    packet: u8,
    /// The value declared in the first packet.
    expected: u8,
    /// The value found in this packet.
    found: u8,
},
```

**Test additions** (`src/dynamic_hdr.rs` `#[cfg(test)]`):
- `decode_sequence_empty_returns_error` — assert `decode_sequence(&[])` returns
  `Err(DecodeError::EmptySequence)`. (Replaces the existing
  `decode_sequence_empty_yields_unknown_zero` test.)
- `decode_sequence_unknown_payload_assembled` — two-packet sequence with known chunk data;
  assert the concatenated payload matches (alloc build).
- `decode_sequence_out_of_order_seq_num_warning` — packet with `seq_num != index`;
  assert `OutOfOrderPacket` warning.
- `decode_sequence_inconsistent_total_bytes_warning` — two packets with differing
  `total_bytes`; assert `InconsistentTotalBytes` warning.
- `decode_sequence_inconsistent_format_id_warning` — two packets with differing
  `format_id`; assert `InconsistentFormatId` warning.

Note: the round-trip test (`decode_sequence_unknown_payload_roundtrip`) depends on
`IntoPackets` and is added in Phase 4.

---

### Phase 2 — HDR10+ struct and decode

**Goal**: parse format identifier `0x04` (HDR10+, ETSI TS 103 433-1) into a typed struct.

#### 2a — Bitstream reader and writer utilities

HDR10+ metadata is a dense bitstream; every field is packed with no byte alignment.
Add private `BitReader<'_>` and `BitWriter` structs in `src/dynamic_hdr.rs`
(or a new `src/bitreader.rs`). Both are needed before Phase 4 encoding; define them
together to avoid a second module-level edit.

**`BitReader`**:

```rust
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,   // next bit to read within data[byte_pos], MSB-first
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self { ... }
    fn read_u8(&mut self, bits: u8) -> Result<u8, DecodeError>   { ... }
    fn read_u16(&mut self, bits: u8) -> Result<u16, DecodeError> { ... }
    fn read_u32(&mut self, bits: u8) -> Result<u32, DecodeError> { ... }
    fn read_bool(&mut self) -> Result<bool, DecodeError>          { ... }
    fn remaining_bits(&self) -> usize                              { ... }
}
```

ETSI TS 103 433-1 §6.1 specifies all fields in MSB-first bit order. Returns
`DecodeError::MalformedPayload` (see Design notes) when the payload runs short.

**`BitWriter`**:

```rust
struct BitWriter {
    buf: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    byte_pos: usize,
    bit_pos: u8,   // next bit to write within buf[byte_pos], MSB-first
}

impl BitWriter {
    fn new() -> Self { ... }
    fn write_u8(&mut self, value: u8, bits: u8)   { ... }
    fn write_u16(&mut self, value: u16, bits: u8) { ... }
    fn write_u32(&mut self, value: u32, bits: u8) { ... }
    fn write_bool(&mut self, value: bool)          { ... }
    /// Returns the populated slice of `buf`.
    fn finish(self) -> ([u8; MAX_DYNAMIC_HDR_PAYLOAD], usize) { ... }
}
```

`BitWriter` always writes into a fixed-size stack buffer (valid in all build
configurations). Alloc builds convert the result into a `Vec<u8>` after `finish`.
Panics on overflow (the caller is responsible for not exceeding `MAX_DYNAMIC_HDR_PAYLOAD`;
this is guaranteed by the struct sizes).

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
#[derive(Default)]
pub struct Hdr10PlusWindows {
    pub count: u8,                         // 1..=3
    pub windows: [Hdr10PlusWindow; 3],     // only `windows[..count]` is valid
}

#[derive(Default)]
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

#[derive(Default)]
pub struct DistributionMaxrgb {
    pub count: u8,                         // up to 15
    pub percentages: [u8; 15],             // 7 bits each
    pub percentiles: [u32; 15],            // 17 bits each
}

pub struct KneePoint {
    pub x: u16,                            // 12 bits
    pub y: u16,                            // 12 bits
}

#[derive(Default)]
pub struct BezierAnchors {
    pub count: u8,                         // up to 9
    pub anchors: [u16; 9],                 // 10 bits each
}

#[derive(Default)]
pub struct ActualPeakLuminance {
    pub num_rows: u8,                      // 5 bits
    pub num_cols: u8,                      // 5 bits
    /// Row-major; only `entries[..num_rows][..num_cols]` is valid.
    /// Values are 4-bit unsigned, representing 0.0–1.0 in steps of 1/15.
    pub entries: [[u8; 25]; 25],
}
```

Derive `Default` on all structs that appear inside `Hdr10PlusWindows` or as array
elements, so that `Hdr10PlusWindows::default()` compiles without heap allocation. `KneePoint` does not need `Default` because it only appears inside `Option<KneePoint>`.

Add `DynamicHdrInfoFrame::Hdr10Plus(Hdr10PlusMetadata)` variant.

Update `decode_sequence` to dispatch on `format_id == 0x04` and call
`Hdr10PlusMetadata::decode(payload: &[u8])`.

**Reserved bits and unknown enum values**: ETSI TS 103 433-1 §6.1 defines several
reserved bits (e.g. the two reserved bits following `application_mode`). During
`Hdr10PlusMetadata::decode`, emit `DynamicHdrWarning::ReservedFieldNonZero { byte, bit }`
for any reserved bit that is set. If `application_mode` carries a value outside {0, 1},
emit `DynamicHdrWarning::UnknownEnumValue { field: "application_mode", raw }` and
continue decoding using the raw value. This defines when `ReservedFieldNonZero` and
`UnknownEnumValue` fire; without these rules they would be dead code.

**Test additions**:
- Unit tests for `BitReader` (exact bit counts, short-read error, MSB-first ordering).
- Unit tests for `BitWriter` (write/read round-trip for each width, MSB-first ordering,
  byte boundary alignment).
- `hdr10plus_round_trip` — construct a `Hdr10PlusMetadata` with known field values,
  encode (Phase 4), decode, assert equality.
- `hdr10plus_single_window_no_peak_lum` — minimal payload, no optional fields set.
- `hdr10plus_three_windows_full` — all optional fields set, three windows.
- `hdr10plus_malformed_short_payload_is_error` — payload too short to hold mandatory
  fields; assert `DecodeError::MalformedPayload`.
- `hdr10plus_reserved_bits_set_warning` — payload with reserved bits set; assert
  `DynamicHdrWarning::ReservedFieldNonZero`.

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

Each variant serializes its metadata into a byte buffer using `BitWriter` (Phase 2a).

**`PayloadBuf` — feature-gated buffer type**: `DynamicHdrIter` stores the serialized
payload in a field that differs by build configuration:

```rust
pub struct DynamicHdrIter {
    format_id: u8,
    total_bytes: u16,
    offset: usize,
    seq_num: u8,

    #[cfg(any(feature = "alloc", feature = "std"))]
    payload: alloc::vec::Vec<u8>,

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload: [u8; MAX_DYNAMIC_HDR_PAYLOAD],

    /// Length of valid bytes in `payload` (bare `no_std` only; `Vec` tracks its own length).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload_len: usize,
}
```

For alloc builds: `BitWriter::finish()` copies into a `Vec<u8>`.
For bare `no_std` builds: the fixed `[u8; MAX_DYNAMIC_HDR_PAYLOAD]` buffer from
`BitWriter::finish()` is moved directly into `DynamicHdrIter`. 600 bytes on the stack
is acceptable in an embedded context.

**`Unknown` in bare `no_std` builds**: `Unknown` has no `payload` field in bare `no_std`
builds. `DynamicHdrInfoFrame::into_packets` must handle this:

```rust
DynamicHdrInfoFrame::Unknown { format_id, .. } => {
    // No payload to encode in bare no_std builds.
    // Return an iterator that yields zero packets.
    // This preserves the existing behaviour of the Phase 3 stub.
    Decoded::new(DynamicHdrIter::empty(format_id))
}
```

In alloc builds `Unknown { format_id, payload }` encodes normally using the stored bytes.
Document `DynamicHdrIter::empty` as a private constructor that sets `total_bytes = 0`
and returns `None` immediately from `next`.

#### 4b — `DynamicHdrIter::next` implementation

**Wire layout for each emitted packet** (matches `DynamicHdrFragment::decode` offsets):

```
Byte  0:    type_code  = 0x20  (Dynamic HDR)
Byte  1:    version    = 0x01
Byte  2:    length     = 4 + chunk_len
Byte  3:    checksum   (computed after filling all other bytes)
Byte  4:    seq_num    (PB0)
Bytes 5–6:  total_bytes little-endian (PB1–2)
Byte  7:    format_id  (PB3)
Bytes 8–30: chunk data (PB4–26; up to 23 bytes; remainder zero)
```

`Iterator::next` implementation:

1. Compute `payload_len` (alloc: `payload.len()`; bare: `self.payload_len`).
2. If `offset >= payload_len`, return `None`.
3. `chunk_len = (payload_len - offset).min(23)`.
4. Build `packet[0..3] = [0x20, 0x01, 4 + chunk_len as u8]`.
5. `packet[4] = seq_num`.
6. `packet[5..7] = total_bytes.to_le_bytes()`.
7. `packet[7] = format_id`.
8. `packet[8..8 + chunk_len]` ← `payload[offset..offset + chunk_len]`.
9. `packet[3] = compute_checksum(&packet[..30])`.
10. `offset += chunk_len; seq_num += 1`.
11. Return `Some(packet)`.

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

Update the `InfoFrame::DynamicHdr` arm in `InfoFrame::into_packets`. This arm cannot use
the `.wrap()` helper directly (the iterator type changes), so it must be written out
explicitly:

```rust
InfoFrame::DynamicHdr(f) => {
    let encoded = f.into_packets(); // Decoded<DynamicHdrIter, DynamicHdrWarning>
    let mut out = Decoded::new(InfoFrameIter(InfoFrameIterInner::Dynamic(encoded.value)));
    for w in encoded.iter_warnings() {
        out.push_warning(Warning::DynamicHdr(w.clone()));
    }
    out
}
```

`DynamicHdrWarning` must implement `Clone` for this (it already derives `Clone`).

Remove the `// Phase 3` placeholder comment.

**Test additions**:
- `decode_sequence_unknown_payload_roundtrip` — encode `Unknown { format_id, payload }`
  via `IntoPackets`, decode back with `decode_sequence`, assert fields equal (alloc build).
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

Recommendation: add `MalformedPayload` in Phase 2, alongside `EmptySequence` from
Phase 1. Both are format-level errors. Update the `error.rs` doc comment for
`DecodeError` to describe both new variants.

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
