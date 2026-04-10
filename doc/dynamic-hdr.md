# Dynamic HDR InfoFrame

Dynamic HDR (CEA-861 type code `0x20`) carries per-frame or per-scene tone-mapping
metadata. Unlike every other InfoFrame type, the payload is variable-length and spread
across a sequence of consecutive packets. Two format identifiers are currently decoded
into typed structs: HDR10+ (`0x04`, ETSI TS 103 433-1) and SL-HDR (`0x02`,
ETSI TS 103 433-1 Table A.1). Unrecognised format identifiers are preserved as
`Unknown`.

## Multi-packet assembly

Each 31-byte packet carries at most 23 bytes of metadata. The top-level `decode`
function returns a `DynamicHdrFragment` for every Dynamic HDR packet it sees; the
caller is responsible for collecting the sequence.

The fragment header provides the fields needed to drive assembly:

| `DynamicHdrFragment` field | Description                                                                      |
|----------------------------|----------------------------------------------------------------------------------|
| `seq_num`                  | Zero-based packet index within the sequence                                      |
| `total_bytes`              | Total metadata byte count declared in the packet header (same across all packets) |
| `format_id`                | Metadata format identifier (same across all packets)                             |
| `chunk`                    | Up to 23 metadata bytes carried by this packet                                   |
| `chunk_len`                | Number of valid bytes in `chunk` (≤ 23)                                          |

The sequence is complete when the sum of `chunk_len` values across all received
fragments reaches `total_bytes`. Once complete, pass the raw `[u8; 31]` packets to
`DynamicHdrInfoFrame::decode_sequence`.

## Decoding

```rust
use cartouche::{decode, dynamic_hdr::DynamicHdrInfoFrame};
use cartouche::frame::InfoFramePacket;

// Accumulate raw packets until the sequence is complete.
let mut raw_packets: Vec<[u8; 31]> = Vec::new();
let mut total_bytes: Option<u16> = None;
let mut chunk_sum: u16 = 0;

for wire_packet in incoming {
    let decoded = decode(&wire_packet)?;
    if let InfoFramePacket::DynamicHdrFragment(frag) = decoded.value {
        total_bytes.get_or_insert(frag.total_bytes);
        chunk_sum += frag.chunk_len as u16;
        raw_packets.push(wire_packet);
        if chunk_sum >= frag.total_bytes {
            break;
        }
    }
}

// Assemble the full frame.
let frame = DynamicHdrInfoFrame::decode_sequence(&raw_packets)?;
match frame.value {
    DynamicHdrInfoFrame::Hdr10Plus(meta) => { /* ... */ }
    DynamicHdrInfoFrame::SlHdr(meta)     => { /* ... */ }
    DynamicHdrInfoFrame::Unknown { format_id, .. } => { /* ... */ }
}
```

## Encoding

```rust
use cartouche::dynamic_hdr::DynamicHdrInfoFrame;
use cartouche::encode::IntoPackets;

let frame = DynamicHdrInfoFrame::Hdr10Plus(meta);
let encoded = frame.into_packets();
for packet in encoded.value {
    transmit(&packet); // each packet is [u8; 31]
}
```

## `DynamicHdrInfoFrame` variants

| Variant              | Format ID | Specification           |
|----------------------|-----------|-------------------------|
| `Hdr10Plus(meta)`    | `0x04`    | ETSI TS 103 433-1 §6.1  |
| `SlHdr(meta)`        | `0x02`    | ETSI TS 103 433-1 §A.1  |
| `Unknown { .. }`     | any other | —                       |

`Unknown` retains the raw payload bytes in `alloc`/`std` builds, making it
re-encodable. In bare `no_std` builds the payload is not retained and
`into_packets` yields zero packets.

## Stack size

`DynamicHdrInfoFrame` is stored inline (no boxing). The `Hdr10Plus` variant is
approximately 1,700 bytes and `SlHdr` approximately 1,100 bytes. Callers who need a
pointer-sized handle can box the whole value at the call site; callers on
stack-constrained targets can store it in a `static`.

## `Hdr10PlusMetadata`

Top-level fields:

| Field                                                    | Bits | Notes                                              |
|----------------------------------------------------------|------|----------------------------------------------------|
| `application_identifier`                                 | 8    |                                                    |
| `application_mode`                                       | 8    | 0 = scene-based, 1 = frame-based; others warn      |
| `scene_frame_switching_flag`                             | 1    | Present in bitstream only when `application_mode == 1` |
| `targeted_system_display_maximum_luminance`              | 27   | cd/m²                                              |
| `targeted_system_display_actual_peak_luminance_flag`     | 1    |                                                    |
| `targeted_system_display_actual_peak_luminance`          | —    | `None` when flag is `false`; see `ActualPeakLuminance` |
| `windows`                                                | —    | 1–3 windows; see `Hdr10PlusWindows`                |
| `mastering_display_actual_peak_luminance_flag`           | 1    |                                                    |
| `mastering_display_actual_peak_luminance`                | —    | `None` when flag is `false`; see `ActualPeakLuminance` |
| `color_saturation_mapping_flag`                          | 1    |                                                    |
| `color_saturation_weight`                                | 6    | `None` when flag is `false`                        |

### `Hdr10PlusWindows`

`windows.count` (1–3) is the number of valid entries in `windows.windows`. Only
`windows.windows[..count as usize]` is populated.

Each `Hdr10PlusWindow`:

| Field                              | Bits | Notes                                            |
|------------------------------------|------|--------------------------------------------------|
| `upper_left_corner_x`              | 16   |                                                  |
| `upper_left_corner_y`              | 16   |                                                  |
| `lower_right_corner_x`             | 16   |                                                  |
| `lower_right_corner_y`             | 16   |                                                  |
| `center_of_ellipse_x`              | 16   |                                                  |
| `center_of_ellipse_y`              | 16   |                                                  |
| `rotation_angle`                   | 8    |                                                  |
| `semimajor_axis_internal_ellipse`  | 16   |                                                  |
| `semimajor_axis_external_ellipse`  | 16   |                                                  |
| `semiminor_axis_external_ellipse`  | 16   |                                                  |
| `overlap_process_option`           | 1    |                                                  |
| `maxscl`                           | 3×17 | Maximum scene-referred linear values (R, G, B)   |
| `average_maxrgb`                   | 17   |                                                  |
| `distribution_maxrgb`              | —    | See `DistributionMaxrgb`                         |
| `fraction_bright_pixels`           | 10   |                                                  |
| `tone_mapping_flag`                | 1    |                                                  |
| `knee_point`                       | —    | `None` when `tone_mapping_flag` is `false`; see `KneePoint` |
| `bezier_curve_anchors`             | —    | Empty when `tone_mapping_flag` is `false`; see `BezierAnchors` |

### `DistributionMaxrgb`

`count` (up to 15) entries; only `percentages[..count]` and `percentiles[..count]`
are valid.

| Field         | Bits per entry |
|---------------|---------------|
| `percentages` | 7             |
| `percentiles` | 17            |

### `KneePoint`

| Field | Bits |
|-------|------|
| `x`   | 12   |
| `y`   | 12   |

### `BezierAnchors`

`count` (up to 9) entries; only `anchors[..count]` is valid. Each anchor is 10 bits.

### `ActualPeakLuminance`

`num_rows` and `num_cols` (up to 25 each); only `entries[..num_rows][..num_cols]` is
valid. Each entry is 4 bits, representing 0–1 in steps of 1/15.

## `SlHdrMetadata`

Top-level fields:

| Field                                         | Bits | Notes                                        |
|-----------------------------------------------|------|----------------------------------------------|
| `itu_t_t35_country_code`                      | 8    |                                              |
| `terminal_provider_code`                      | 16   |                                              |
| `terminal_provider_oriented_code_message_idc` | 8    |                                              |
| `sl_hdr_mode_value_minus1`                    | 4    | Stored as mode − 1                           |
| `sl_hdr_spec_major_version_idc`               | 4    |                                              |
| `sl_hdr_spec_minor_version_idc`               | 7    |                                              |
| `sl_hdr_cancel_flag`                          | 1    | When `true`, all parameters are cancelled and `body` is `None` |
| `body`                                        | —    | `None` when `sl_hdr_cancel_flag` is `true`; see `SlHdrBody` |

### `SlHdrBody`

| Field                        | Bits | Notes                                                     |
|------------------------------|------|-----------------------------------------------------------|
| `sl_hdr_persistence_flag`    | 1    |                                                           |
| `sl_hdr_payload_mode`        | 3    | 0 = tone-mapping, 1 = luminance/colour mapping; others warn |
| `original_picture_info`      | —    | `None` when not present; see `SlHdrPictureInfo`           |
| `target_picture_info`        | —    | `None` when not present; see `SlHdrPictureInfo`           |
| `src_mdcv_info`              | —    | `None` when not present; see `SlHdrMdcvInfo`              |
| `matrix_coefficient_values`  | 4×16 |                                                           |
| `chroma_to_luma_injection`   | 2×16 |                                                           |
| `k_coefficient_values`       | 3×8  |                                                           |
| `payload`                    | —    | See `SlHdrPayload`                                        |
| `extension`                  | —    | `alloc`/`std` builds only; see note below                 |

`extension` is `None` in bare `no_std` builds — the extension bytes are not retained,
so encoding a decoded extension-carrying stream is lossy in that configuration.

### `SlHdrPictureInfo`

| Field           | Bits |
|-----------------|------|
| `primaries`     | 8    |
| `max_luminance` | 16   |
| `min_luminance` | 16   |

### `SlHdrMdcvInfo`

| Field                   | Bits  | Notes                              |
|-------------------------|-------|------------------------------------|
| `primaries`             | 3×2×16 | Chromaticity x/y for 3 primaries  |
| `ref_white_x`           | 16    |                                    |
| `ref_white_y`           | 16    |                                    |
| `max_mastering_luminance` | 16  |                                    |
| `min_mastering_luminance` | 16  |                                    |

### `SlHdrPayload`

| Variant         | `sl_hdr_payload_mode` | Description                              |
|-----------------|-----------------------|------------------------------------------|
| `Mode0(..)`     | 0                     | Tone-mapping tables; see `SlHdrMode0`    |
| `Mode1(..)`     | 1                     | Luminance/colour mapping; see `SlHdrMode1` |
| `Unknown(u8)`   | any other             | Raw mode byte preserved; encode is lossy |

### `SlHdrMode0`

| Field                                          | Bits | Notes                            |
|------------------------------------------------|------|----------------------------------|
| `tone_mapping_input_signal_black_level_offset` | 8    |                                  |
| `tone_mapping_input_signal_white_level_offset` | 8    |                                  |
| `shadow_gain_control`                          | 8    |                                  |
| `highlight_gain_control`                       | 8    |                                  |
| `mid_tone_width_adjustment_factor`             | 8    |                                  |
| `tone_mapping_output_fine_tuning`              | —    | Up to 15 (x, y) byte pairs; see `SlHdrTable15` |
| `saturation_gain`                              | —    | Up to 15 (x, y) byte pairs; see `SlHdrTable15` |

### `SlHdrMode1`

| Field                       | Bits | Notes                                                         |
|-----------------------------|------|---------------------------------------------------------------|
| `lm_uniform_sampling_flag`  | 1    | When `true`, `luminance_mapping.x` is absent from the bitstream and holds zeros |
| `luminance_mapping`         | —    | Up to 127 (x, y) u16 pairs; see `SlHdrTable127`              |
| `cc_uniform_sampling_flag`  | 1    | When `true`, `colour_correction.x` is absent from the bitstream and holds zeros |
| `colour_correction`         | —    | Up to 127 (x, y) u16 pairs; see `SlHdrTable127`              |

### `SlHdrTable15` / `SlHdrTable127`

Both tables use a `count` field (4 bits for `SlHdrTable15`, 7 bits for `SlHdrTable127`)
and parallel `x` / `y` arrays; only `[..count]` is valid. `SlHdrTable15` entries are
`u8`; `SlHdrTable127` entries are `u16`.

## Warnings

Warnings from fragment decoding and sequence assembly are returned on the
`Decoded<DynamicHdrInfoFrame, DynamicHdrWarning>` wrapper. Up to 4 format-level warnings
(from HDR10+ or SL-HDR payload parsing) are forwarded; further warnings are silently
dropped.

| Variant                        | Meaning                                                                                  |
|--------------------------------|------------------------------------------------------------------------------------------|
| `ChecksumMismatch { expected, found }` | A packet's checksum byte does not make the 31-byte sum equal zero. Decoding continues. |
| `ReservedFieldNonZero { byte, bit }` | A reserved bit in the payload was set. Decoding continues.                         |
| `UnknownEnumValue { field, raw }` | A field carried a value outside the defined set. Decoding continues with the raw value. |
| `OutOfOrderPacket { index, found }` | A packet's `seq_num` did not match its position in the sequence slice.             |
| `InconsistentTotalBytes { packet, expected, found }` | A packet's `total_bytes` header field differs from the first packet's value. |
| `InconsistentFormatId { packet, expected, found }` | A packet's `format_id` header field differs from the first packet's value.   |

## Fragment wire layout

Each 31-byte Dynamic HDR packet has the following structure:

```
Byte  0:     type_code = 0x20
Byte  1:     version   = 0x01
Byte  2:     length    = 4 + chunk_len  (chunk_len ≤ 23; length > 27 is a DecodeError)
Byte  3:     checksum  (sum of all 31 bytes must be 0x00 mod 256)
Byte  4:     seq_num   (PB0 — zero-based packet index)
Bytes 5–6:   total_bytes little-endian (PB1–2)
Byte  7:     format_id (PB3)
Bytes 8–30:  metadata chunk (PB4–26; bytes 8..8+chunk_len carry data; remainder zero)
```
