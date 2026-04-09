use crate::decoded::Decoded;
use crate::encode::IntoPackets;
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;

/// Maximum byte length of an assembled Dynamic HDR metadata payload.
///
/// Sized for a worst-case SL-HDR mode-1 + max extension fields populated: approximately 2107 bytes.
/// Used as the stack-buffer size in bare `no_std` builds where heap allocation is unavailable.
pub(crate) const MAX_DYNAMIC_HDR_PAYLOAD: usize = 2200;

/// MSB-first bit-stream reader.
///
/// Used to parse HDR10+ and SL-HDR payloads, whose fields are bit-packed
/// with no byte alignment (ETSI TS 103 433-1 §6.1).
struct BitReader<'a> {
    data: &'a [u8],
    /// Index of the byte currently being read.
    byte_pos: usize,
    /// Next bit to read within `data[byte_pos]`, counting from the MSB.
    /// 0 = MSB (bit 7), 7 = LSB (bit 0). After bit 7 the reader advances to
    /// the next byte.
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read `bits` bits (1–8) and return them right-aligned in a `u8`.
    fn read_u8(&mut self, bits: u8) -> Result<u8, DecodeError> {
        debug_assert!((1..=8).contains(&bits));
        Ok(self.read_bits(bits)? as u8)
    }

    /// Read `bits` bits (1–16) and return them right-aligned in a `u16`.
    fn read_u16(&mut self, bits: u8) -> Result<u16, DecodeError> {
        debug_assert!((1..=16).contains(&bits));
        Ok(self.read_bits(bits)? as u16)
    }

    /// Read `bits` bits (1–32) and return them right-aligned in a `u32`.
    fn read_u32(&mut self, bits: u8) -> Result<u32, DecodeError> {
        debug_assert!((1..=32).contains(&bits));
        self.read_bits(bits)
    }

    /// Read a single bit as a `bool`.
    fn read_bool(&mut self) -> Result<bool, DecodeError> {
        Ok(self.read_bits(1)? != 0)
    }

    /// Number of bits remaining in the buffer.
    fn remaining_bits(&self) -> usize {
        let remaining_bytes = self.data.len().saturating_sub(self.byte_pos);
        remaining_bytes * 8 - self.bit_pos as usize
    }

    /// Core read: consumes `n` bits MSB-first and returns them in the low bits of a `u32`.
    fn read_bits(&mut self, mut n: u8) -> Result<u32, DecodeError> {
        if self.remaining_bits() < n as usize {
            return Err(DecodeError::MalformedPayload);
        }
        let mut result: u32 = 0;
        while n > 0 {
            // Bits available in the current byte.
            let avail = 8 - self.bit_pos;
            let take = n.min(avail);
            // Shift the current byte so the next `take` bits are at the top,
            // then mask them off.
            let shift = avail - take;
            let mask = ((1u16 << take) - 1) as u8;
            let bits = (self.data[self.byte_pos] >> shift) & mask;
            result = (result << take) | bits as u32;
            self.bit_pos += take;
            if self.bit_pos == 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
            n -= take;
        }
        Ok(result)
    }
}

/// MSB-first bit-stream writer.
///
/// Packs fields into a fixed-size stack buffer for encoding HDR10+ and SL-HDR
/// payloads. Panics on overflow — callers must not exceed
/// `MAX_DYNAMIC_HDR_PAYLOAD` bytes.
struct BitWriter {
    buf: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    /// Index of the byte currently being written.
    byte_pos: usize,
    /// Next bit to write within `buf[byte_pos]`, counting from the MSB.
    /// 0 = MSB (bit 7), 7 = LSB (bit 0).
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            buf: [0u8; MAX_DYNAMIC_HDR_PAYLOAD],
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Write `bits` bits (1–8) from the low bits of `value`.
    fn write_u8(&mut self, value: u8, bits: u8) {
        debug_assert!((1..=8).contains(&bits));
        self.write_bits(value as u32, bits);
    }

    /// Write `bits` bits (1–16) from the low bits of `value`.
    fn write_u16(&mut self, value: u16, bits: u8) {
        debug_assert!((1..=16).contains(&bits));
        self.write_bits(value as u32, bits);
    }

    /// Write `bits` bits (1–32) from the low bits of `value`.
    fn write_u32(&mut self, value: u32, bits: u8) {
        debug_assert!((1..=32).contains(&bits));
        self.write_bits(value, bits);
    }

    /// Write a single bit.
    fn write_bool(&mut self, value: bool) {
        self.write_bits(value as u32, 1);
    }

    /// Returns the populated slice of the buffer.
    fn finish(self) -> ([u8; MAX_DYNAMIC_HDR_PAYLOAD], usize) {
        let len = if self.bit_pos == 0 {
            self.byte_pos
        } else {
            self.byte_pos + 1
        };
        (self.buf, len)
    }

    /// Core write: places the low `n` bits of `value` into the buffer MSB-first.
    fn write_bits(&mut self, value: u32, mut n: u8) {
        assert!(
            self.byte_pos < MAX_DYNAMIC_HDR_PAYLOAD,
            "BitWriter overflow"
        );
        while n > 0 {
            let avail = 8 - self.bit_pos;
            let take = n.min(avail);
            // Extract the top `take` bits from the remaining `n` bits of value.
            let shift = n - take;
            let bits = ((value >> shift) as u8) & (((1u16 << take) - 1) as u8);
            // Place them at the correct position within the current byte.
            self.buf[self.byte_pos] |= bits << (avail - take);
            self.bit_pos += take;
            if self.bit_pos == 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
            n -= take;
        }
    }
}

// ---------------------------------------------------------------------------
// HDR10+ metadata types (ETSI TS 103 433-1)
// ---------------------------------------------------------------------------

/// HDR10+ dynamic metadata (ETSI TS 103 433-1, format identifier `0x04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hdr10PlusMetadata {
    /// Application-specific identifier (8 bits).
    pub application_identifier: u8,
    /// Application mode: 0 = scene-based, 1 = frame-based (8 bits).
    pub application_mode: u8,
    /// Scene/frame switching flag — present only when `application_mode == 1`
    /// (1 bit).
    pub scene_frame_switching_flag: bool,
    /// Maximum luminance of the targeted system display, in cd/m² (27 bits).
    pub targeted_system_display_maximum_luminance: u32,
    /// Whether the targeted system display actual peak luminance table is
    /// present (1 bit).
    pub targeted_system_display_actual_peak_luminance_flag: bool,
    /// Targeted system display actual peak luminance table.
    /// `None` when the flag above is `false`.
    pub targeted_system_display_actual_peak_luminance: Option<ActualPeakLuminance>,
    /// Tone-mapping window data (1–3 windows).
    pub windows: Hdr10PlusWindows,
    /// Whether the mastering display actual peak luminance table is present
    /// (1 bit).
    pub mastering_display_actual_peak_luminance_flag: bool,
    /// Mastering display actual peak luminance table.
    /// `None` when the flag above is `false`.
    pub mastering_display_actual_peak_luminance: Option<ActualPeakLuminance>,
    /// Whether colour saturation mapping is present (1 bit).
    pub color_saturation_mapping_flag: bool,
    /// Colour saturation weight (6 bits). `None` when
    /// `color_saturation_mapping_flag` is `false`.
    pub color_saturation_weight: Option<u8>,
}

/// Up to three tone-mapping windows.
///
/// Stored as a fixed array with a count rather than a `Vec` to allow `no_std`
/// without alloc. Only `windows[..count as usize]` is valid.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hdr10PlusWindows {
    /// Number of valid windows (1–3).
    pub count: u8,
    /// Window data; only `windows[..count as usize]` is populated.
    pub windows: [Hdr10PlusWindow; 3],
}

/// Tone-mapping parameters for one window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hdr10PlusWindow {
    /// Upper-left corner X coordinate (16 bits).
    pub upper_left_corner_x: u16,
    /// Upper-left corner Y coordinate (16 bits).
    pub upper_left_corner_y: u16,
    /// Lower-right corner X coordinate (16 bits).
    pub lower_right_corner_x: u16,
    /// Lower-right corner Y coordinate (16 bits).
    pub lower_right_corner_y: u16,
    /// Centre of ellipse X coordinate (16 bits).
    pub center_of_ellipse_x: u16,
    /// Centre of ellipse Y coordinate (16 bits).
    pub center_of_ellipse_y: u16,
    /// Rotation angle of the ellipse (8 bits).
    pub rotation_angle: u8,
    /// Semi-major axis of the internal ellipse (16 bits).
    pub semimajor_axis_internal_ellipse: u16,
    /// Semi-major axis of the external ellipse (16 bits).
    pub semimajor_axis_external_ellipse: u16,
    /// Semi-minor axis of the external ellipse (16 bits).
    pub semiminor_axis_external_ellipse: u16,
    /// Overlap process option (1 bit).
    pub overlap_process_option: bool,
    /// Maximum Scene-referred Linear values, one per RGB component (3 × 17
    /// bits).
    pub maxscl: [u32; 3],
    /// Average maximum RGB value (17 bits).
    pub average_maxrgb: u32,
    /// MaxRGB distribution data.
    pub distribution_maxrgb: DistributionMaxrgb,
    /// Fraction of bright pixels (10 bits).
    pub fraction_bright_pixels: u16,
    /// Whether tone-mapping parameters are present (1 bit).
    pub tone_mapping_flag: bool,
    /// Knee point for the tone-mapping curve.
    /// `None` when `tone_mapping_flag` is `false`.
    pub knee_point: Option<KneePoint>,
    /// Bezier curve anchors for the tone-mapping curve.
    /// Only populated when `tone_mapping_flag` is `true`.
    pub bezier_curve_anchors: BezierAnchors,
}

/// MaxRGB distribution percentages and percentiles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DistributionMaxrgb {
    /// Number of valid distribution entries (up to 15).
    pub count: u8,
    /// Percentage values (7 bits each); only `[..count as usize]` is valid.
    pub percentages: [u8; 15],
    /// Percentile values (17 bits each); only `[..count as usize]` is valid.
    pub percentiles: [u32; 15],
}

/// Knee-point coordinates for the tone-mapping curve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KneePoint {
    /// X coordinate (12 bits).
    pub x: u16,
    /// Y coordinate (12 bits).
    pub y: u16,
}

/// Bezier curve anchors for the tone-mapping curve.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BezierAnchors {
    /// Number of valid anchors (up to 9).
    pub count: u8,
    /// Anchor values (10 bits each); only `[..count as usize]` is valid.
    pub anchors: [u16; 9],
}

/// Actual peak luminance table, used for both targeted system display and
/// mastering display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActualPeakLuminance {
    /// Number of rows (5 bits, up to 25).
    pub num_rows: u8,
    /// Number of columns (5 bits, up to 25).
    pub num_cols: u8,
    /// Row-major luminance entries (4 bits each, representing 0–1 in steps of
    /// 1/15). Only `entries[..num_rows][..num_cols]` is valid.
    pub entries: [[u8; 25]; 25],
}

// ---------------------------------------------------------------------------
// SL-HDR metadata types (ETSI TS 103 433-1 Table A.1)
// ---------------------------------------------------------------------------

/// SL-HDR dynamic metadata (ETSI TS 103 433-1 Table A.1, format identifier
/// `0x02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrMetadata {
    /// `itu_t_t35_country_code` (8 bits).
    pub itu_t_t35_country_code: u8,
    /// `terminal_provider_code` (16 bits).
    pub terminal_provider_code: u16,
    /// `terminal_provider_oriented_code_message_idc` (8 bits).
    pub terminal_provider_oriented_code_message_idc: u8,
    /// `sl_hdr_mode_value_minus1` (4 bits).
    pub sl_hdr_mode_value_minus1: u8,
    /// `sl_hdr_spec_major_version_idc` (4 bits).
    pub sl_hdr_spec_major_version_idc: u8,
    /// `sl_hdr_spec_minor_version_idc` (7 bits).
    pub sl_hdr_spec_minor_version_idc: u8,
    /// When `true`, all SL-HDR parameters are cancelled and `body` is `None`.
    pub sl_hdr_cancel_flag: bool,
    /// Present only when `sl_hdr_cancel_flag` is `false`.
    pub body: Option<SlHdrBody>,
}

/// Main SL-HDR parameter block, present when `sl_hdr_cancel_flag` is `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrBody {
    /// `sl_hdr_persistence_flag`.
    pub sl_hdr_persistence_flag: bool,
    /// `sl_hdr_payload_mode` (3 bits): 0 = tone-mapping, 1 = luminance/colour
    /// mapping.
    pub sl_hdr_payload_mode: u8,
    /// Present when `original_picture_info_present_flag` is set.
    pub original_picture_info: Option<SlHdrPictureInfo>,
    /// Present when `target_picture_info_present_flag` is set.
    pub target_picture_info: Option<SlHdrPictureInfo>,
    /// Present when `src_mdcv_info_present_flag` is set.
    pub src_mdcv_info: Option<SlHdrMdcvInfo>,
    /// `matrix_coefficient_value[0..4]` (4 × 16 bits).
    pub matrix_coefficient_values: [u16; 4],
    /// `chroma_to_luma_injection[0..2]` (2 × 16 bits).
    pub chroma_to_luma_injection: [u16; 2],
    /// `k_coefficient_value[0..3]` (3 × 8 bits).
    pub k_coefficient_values: [u8; 3],
    /// Mode-dependent payload data.
    pub payload: SlHdrPayload,
    /// Raw extension bytes, present when `sl_hdr_extension_present_flag` was
    /// set. Only retained in `alloc`/`std` builds.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub extension: Option<SlHdrExtension>,
}

/// Picture colour and luminance info (original or target picture).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrPictureInfo {
    /// `*_picture_primaries` (8 bits).
    pub primaries: u8,
    /// `*_picture_max_luminance` (16 bits).
    pub max_luminance: u16,
    /// `*_picture_min_luminance` (16 bits).
    pub min_luminance: u16,
}

/// Source mastering display colour volume info.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlHdrMdcvInfo {
    /// Chromaticity coordinates `[component][0=x, 1=y]` for 3 primaries
    /// (3 × 2 × 16 bits).
    pub primaries: [[u16; 2]; 3],
    /// `src_mdcv_ref_white_x` (16 bits).
    pub ref_white_x: u16,
    /// `src_mdcv_ref_white_y` (16 bits).
    pub ref_white_y: u16,
    /// `src_mdcv_max_mastering_luminance` (16 bits).
    pub max_mastering_luminance: u16,
    /// `src_mdcv_min_mastering_luminance` (16 bits).
    pub min_mastering_luminance: u16,
}

/// Mode-dependent payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlHdrPayload {
    /// `sl_hdr_payload_mode == 0`: tone-mapping tables.
    Mode0(SlHdrMode0),
    /// `sl_hdr_payload_mode == 1`: luminance/colour mapping tables.
    ///
    /// Heap-allocated to keep enum variant sizes comparable; `SlHdrMode1`
    /// contains two 127-entry tables. Only available in `alloc`/`std` builds.
    #[cfg(any(feature = "alloc", feature = "std"))]
    Mode1(alloc::boxed::Box<SlHdrMode1>),
    /// Any other `sl_hdr_payload_mode` value; the raw mode byte is preserved.
    Unknown(u8),
}

/// Tone-mapping payload (`sl_hdr_payload_mode == 0`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlHdrMode0 {
    /// `tone_mapping_input_signal_black_level_offset` (8 bits).
    pub tone_mapping_input_signal_black_level_offset: u8,
    /// `tone_mapping_input_signal_white_level_offset` (8 bits).
    pub tone_mapping_input_signal_white_level_offset: u8,
    /// `shadow_gain_control` (8 bits).
    pub shadow_gain_control: u8,
    /// `highlight_gain_control` (8 bits).
    pub highlight_gain_control: u8,
    /// `mid_tone_width_adjustment_factor` (8 bits).
    pub mid_tone_width_adjustment_factor: u8,
    /// `tone_mapping_output_fine_tuning` table (up to 15 entries; count is 4
    /// bits).
    pub tone_mapping_output_fine_tuning: SlHdrTable15,
    /// `saturation_gain` table (up to 15 entries; count is 4 bits).
    pub saturation_gain: SlHdrTable15,
}

/// Luminance/colour mapping payload (`sl_hdr_payload_mode == 1`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlHdrMode1 {
    /// `lm_uniform_sampling_flag`. When `true`, `luminance_mapping.x` is not
    /// present in the bitstream and holds zeros.
    pub lm_uniform_sampling_flag: bool,
    /// Luminance mapping table (up to 127 entries; count is 7 bits).
    pub luminance_mapping: SlHdrTable127,
    /// `cc_uniform_sampling_flag`. When `true`, `colour_correction.x` is not
    /// present in the bitstream and holds zeros.
    pub cc_uniform_sampling_flag: bool,
    /// Colour correction table (up to 127 entries; count is 7 bits).
    pub colour_correction: SlHdrTable127,
}

/// A table of up to 15 (x, y) byte pairs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlHdrTable15 {
    /// Number of valid entries (up to 15).
    pub count: u8,
    /// X values; only `x[..count]` is valid.
    pub x: [u8; 15],
    /// Y values; only `y[..count]` is valid.
    pub y: [u8; 15],
}

/// A table of up to 127 (x, y) u16 pairs.
///
/// X values are absent from the bitstream (and zeroed here) when the
/// corresponding `*_uniform_sampling_flag` is `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrTable127 {
    /// Number of valid entries (up to 127).
    pub count: u8,
    /// X values; only `x[..count]` is valid.
    pub x: [u16; 127],
    /// Y values; only `y[..count]` is valid.
    pub y: [u16; 127],
}

impl Default for SlHdrTable127 {
    fn default() -> Self {
        Self {
            count: 0,
            x: [0u16; 127],
            y: [0u16; 127],
        }
    }
}

/// Raw extension data from `sl_hdr_extension_*` fields.
#[cfg(any(feature = "alloc", feature = "std"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrExtension {
    /// `sl_hdr_extension_6bits` (6 bits).
    pub extension_6bits: u8,
    /// Raw `sl_hdr_extension_data_byte` bytes.
    pub data: alloc::vec::Vec<u8>,
}

/// A Dynamic HDR InfoFrame.
///
/// Carries per-frame or per-scene dynamic tone mapping metadata for formats
/// including HDR10+ (ETSI TS 103 433) and SL-HDR. Unlike all other InfoFrame
/// types, the payload is variable length and spans multiple packets.
///
/// Use [`DynamicHdrFragment::decode`] to decode individual packets as they
/// arrive. Once the full sequence is assembled, pass the raw packets to
/// [`DynamicHdrInfoFrame::decode_sequence`] to obtain this type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DynamicHdrInfoFrame {
    /// An unrecognised metadata format.
    ///
    /// Returned when the format identifier in the packet sequence is not
    /// recognised by this version of `cartouche`. In `alloc`/`std` builds the
    /// raw metadata bytes are retained in `payload`, making this variant
    /// re-encodable. In bare `no_std` builds the payload is not retained.
    /// HDR10+ dynamic metadata (ETSI TS 103 433-1, format identifier `0x04`).
    ///
    /// The metadata is heap-allocated to keep the enum size comparable to
    /// other variants. Only available in `alloc`/`std` builds; bare `no_std`
    /// builds decode format `0x04` as [`Unknown`](DynamicHdrInfoFrame::Unknown).
    #[cfg(any(feature = "alloc", feature = "std"))]
    Hdr10Plus(alloc::boxed::Box<Hdr10PlusMetadata>),
    /// SL-HDR dynamic metadata (ETSI TS 103 433-1 Table A.1, format identifier
    /// `0x02`).
    ///
    /// Heap-allocated for the same reason as `Hdr10Plus`. Only available in
    /// `alloc`/`std` builds; bare `no_std` builds decode format `0x02` as
    /// [`Unknown`](DynamicHdrInfoFrame::Unknown).
    #[cfg(any(feature = "alloc", feature = "std"))]
    SlHdr(alloc::boxed::Box<SlHdrMetadata>),
    /// An unrecognised metadata format.
    ///
    /// Returned when the format identifier in the packet sequence is not
    /// recognised by this version of `cartouche`. In `alloc`/`std` builds the
    /// raw metadata bytes are retained in `payload`, making this variant
    /// re-encodable. In bare `no_std` builds the payload is not retained.
    Unknown {
        /// The metadata format identifier from the first packet in the sequence.
        format_id: u8,
        /// Raw metadata bytes concatenated from all chunks in the sequence.
        ///
        /// Only present in `alloc`/`std` builds. In bare `no_std` builds the
        /// payload is not retained.
        #[cfg(any(feature = "alloc", feature = "std"))]
        payload: alloc::vec::Vec<u8>,
    },
}

impl DynamicHdrInfoFrame {
    /// Assemble a [`DynamicHdrInfoFrame`] from a complete sequence of wire packets.
    ///
    /// The caller is responsible for collecting the sequence. When the sum of
    /// `chunk_len` values across all [`DynamicHdrFragment`]s received via the
    /// top-level [`decode`](crate::decode) function equals `total_bytes`, the
    /// sequence is complete and ready to pass here.
    ///
    /// The format identifier is read from the first packet in the sequence
    /// (byte 7, PB3). Unknown format identifiers produce
    /// [`DynamicHdrInfoFrame::Unknown`]; the raw payload bytes are not retained.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if any packet in `packets` has
    /// `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`] — for any packet whose
    ///   checksum does not verify.
    pub fn decode_sequence(
        packets: &[[u8; 31]],
    ) -> Result<Decoded<DynamicHdrInfoFrame, DynamicHdrWarning>, DecodeError> {
        if packets.is_empty() {
            return Err(DecodeError::EmptySequence);
        }

        // Read sequence-level invariants from the first packet.
        let format_id = packets[0][7];
        let total_bytes = u16::from_le_bytes([packets[0][5], packets[0][6]]);

        let mut decoded = Decoded::new(DynamicHdrInfoFrame::Unknown {
            format_id,
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload: alloc::vec::Vec::new(),
        });

        #[cfg(any(feature = "alloc", feature = "std"))]
        let mut payload: alloc::vec::Vec<u8> = alloc::vec::Vec::new();

        for (i, packet) in packets.iter().enumerate() {
            let frag = DynamicHdrFragment::decode(packet)?;
            for w in frag.iter_warnings() {
                decoded.push_warning(w.clone());
            }

            // Sequence integrity.
            if frag.value.seq_num != i as u8 {
                decoded.push_warning(DynamicHdrWarning::OutOfOrderPacket {
                    index: i as u8,
                    found: frag.value.seq_num,
                });
            }
            // Consistency against the first packet's invariants (skip i==0: trivially equal).
            if i > 0 {
                if frag.value.total_bytes != total_bytes {
                    decoded.push_warning(DynamicHdrWarning::InconsistentTotalBytes {
                        packet: i as u8,
                        expected: total_bytes,
                        found: frag.value.total_bytes,
                    });
                }
                if frag.value.format_id != format_id {
                    decoded.push_warning(DynamicHdrWarning::InconsistentFormatId {
                        packet: i as u8,
                        expected: format_id,
                        found: frag.value.format_id,
                    });
                }
            }

            // Chunk accumulation.
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload.extend_from_slice(&frag.value.chunk[..frag.value.chunk_len as usize]);
        }

        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            let mut format_warnings: alloc::vec::Vec<DynamicHdrWarning> = alloc::vec::Vec::new();
            decoded.value = match format_id {
                0x02 => match SlHdrMetadata::decode(&payload, &mut |w| format_warnings.push(w)) {
                    Ok(meta) => DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(meta)),
                    Err(e) => return Err(e),
                },
                0x04 => match Hdr10PlusMetadata::decode(&payload, &mut |w| format_warnings.push(w))
                {
                    Ok(meta) => DynamicHdrInfoFrame::Hdr10Plus(alloc::boxed::Box::new(meta)),
                    Err(e) => return Err(e),
                },
                _ => DynamicHdrInfoFrame::Unknown { format_id, payload },
            };
            for w in format_warnings {
                decoded.push_warning(w);
            }
        }

        Ok(decoded)
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl Hdr10PlusMetadata {
    /// Parse a HDR10+ metadata payload (ETSI TS 103 433-1 §6.1).
    ///
    /// `push_warning` is called for each non-fatal anomaly encountered (reserved
    /// bits set, unrecognised `application_mode` value).
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::MalformedPayload`] if `payload` is too short to
    /// hold the mandatory fields for the declared structure.
    pub fn decode(
        payload: &[u8],
        push_warning: &mut impl FnMut(DynamicHdrWarning),
    ) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(payload);

        let application_identifier = r.read_u8(8)?;
        let application_mode = r.read_u8(8)?;

        if application_mode > 1 {
            push_warning(DynamicHdrWarning::UnknownEnumValue {
                field: "application_mode",
                raw: application_mode,
            });
        }

        let scene_frame_switching_flag = if application_mode == 1 {
            r.read_bool()?
        } else {
            false
        };

        // Two reserved bits following application_mode (and optional switching flag).
        for _ in 0..2 {
            let byte_idx = r.byte_pos as u8;
            let bit_idx = 7 - r.bit_pos; // convert MSB-first pos → conventional bit number
            if r.read_bool()? {
                push_warning(DynamicHdrWarning::ReservedFieldNonZero {
                    byte: byte_idx,
                    bit: bit_idx,
                });
            }
        }

        let targeted_system_display_maximum_luminance = r.read_u32(27)?;

        let targeted_system_display_actual_peak_luminance_flag = r.read_bool()?;
        let targeted_system_display_actual_peak_luminance =
            if targeted_system_display_actual_peak_luminance_flag {
                Some(Self::decode_actual_peak_luminance(&mut r)?)
            } else {
                None
            };

        // num_windows is stored as (count − 1) in 2 bits. Raw values 0–2 give
        // 1–3 windows; raw value 3 (4 windows) exceeds the array capacity and
        // is rejected as a malformed payload.
        let num_windows_minus1 = r.read_u8(2)?;
        if num_windows_minus1 > 2 {
            return Err(DecodeError::MalformedPayload);
        }
        let num_windows = num_windows_minus1 + 1;
        let mut windows = Hdr10PlusWindows {
            count: num_windows,
            ..Default::default()
        };
        for i in 0..num_windows as usize {
            windows.windows[i] = Self::decode_window(&mut r)?;
        }

        let mastering_display_actual_peak_luminance_flag = r.read_bool()?;
        let mastering_display_actual_peak_luminance =
            if mastering_display_actual_peak_luminance_flag {
                Some(Self::decode_actual_peak_luminance(&mut r)?)
            } else {
                None
            };

        // Tone-mapping data is read in a second pass over the windows.
        for i in 0..num_windows as usize {
            let tone_mapping_flag = r.read_bool()?;
            windows.windows[i].tone_mapping_flag = tone_mapping_flag;
            if tone_mapping_flag {
                let x = r.read_u16(12)?;
                let y = r.read_u16(12)?;
                windows.windows[i].knee_point = Some(KneePoint { x, y });
                let num_anchors = r.read_u8(4)?;
                if num_anchors > 9 {
                    return Err(DecodeError::MalformedPayload);
                }
                windows.windows[i].bezier_curve_anchors.count = num_anchors;
                for j in 0..num_anchors as usize {
                    windows.windows[i].bezier_curve_anchors.anchors[j] = r.read_u16(10)?;
                }
            }
        }

        let color_saturation_mapping_flag = r.read_bool()?;
        let color_saturation_weight = if color_saturation_mapping_flag {
            Some(r.read_u8(6)?)
        } else {
            None
        };

        Ok(Hdr10PlusMetadata {
            application_identifier,
            application_mode,
            scene_frame_switching_flag,
            targeted_system_display_maximum_luminance,
            targeted_system_display_actual_peak_luminance_flag,
            targeted_system_display_actual_peak_luminance,
            windows,
            mastering_display_actual_peak_luminance_flag,
            mastering_display_actual_peak_luminance,
            color_saturation_mapping_flag,
            color_saturation_weight,
        })
    }

    fn decode_actual_peak_luminance(
        r: &mut BitReader<'_>,
    ) -> Result<ActualPeakLuminance, DecodeError> {
        let num_rows = r.read_u8(5)?;
        let num_cols = r.read_u8(5)?;
        let mut entries = [[0u8; 25]; 25];
        for row in entries.iter_mut().take(num_rows as usize) {
            for entry in row.iter_mut().take(num_cols as usize) {
                *entry = r.read_u8(4)?;
            }
        }
        Ok(ActualPeakLuminance {
            num_rows,
            num_cols,
            entries,
        })
    }

    /// Serialize this metadata to a byte buffer using `BitWriter`.
    ///
    /// Returns `(buf, len)` — the populated prefix of `buf`.
    pub(crate) fn encode(&self) -> ([u8; MAX_DYNAMIC_HDR_PAYLOAD], usize) {
        let mut w = BitWriter::new();

        w.write_u8(self.application_identifier, 8);
        w.write_u8(self.application_mode, 8);
        if self.application_mode == 1 {
            w.write_bool(self.scene_frame_switching_flag);
        }
        w.write_u8(0, 2); // 2 reserved bits

        w.write_u32(self.targeted_system_display_maximum_luminance, 27);
        w.write_bool(self.targeted_system_display_actual_peak_luminance_flag);
        if let Some(ref lum) = self.targeted_system_display_actual_peak_luminance {
            Self::encode_actual_peak_luminance(&mut w, lum);
        }

        w.write_u8(self.windows.count - 1, 2); // stored as count − 1
        for win in self.windows.windows[..self.windows.count as usize].iter() {
            Self::encode_window(&mut w, win);
        }

        w.write_bool(self.mastering_display_actual_peak_luminance_flag);
        if let Some(ref lum) = self.mastering_display_actual_peak_luminance {
            Self::encode_actual_peak_luminance(&mut w, lum);
        }

        // Tone-mapping second pass.
        for win in self.windows.windows[..self.windows.count as usize].iter() {
            w.write_bool(win.tone_mapping_flag);
            if win.tone_mapping_flag {
                if let Some(ref kp) = win.knee_point {
                    w.write_u16(kp.x, 12);
                    w.write_u16(kp.y, 12);
                }
                w.write_u8(win.bezier_curve_anchors.count, 4);
                for &anchor in win.bezier_curve_anchors.anchors
                    [..win.bezier_curve_anchors.count as usize]
                    .iter()
                {
                    w.write_u16(anchor, 10);
                }
            }
        }

        w.write_bool(self.color_saturation_mapping_flag);
        if let Some(weight) = self.color_saturation_weight {
            w.write_u8(weight, 6);
        }

        w.finish()
    }

    fn encode_actual_peak_luminance(w: &mut BitWriter, lum: &ActualPeakLuminance) {
        w.write_u8(lum.num_rows, 5);
        w.write_u8(lum.num_cols, 5);
        for row in lum.entries.iter().take(lum.num_rows as usize) {
            for &entry in row.iter().take(lum.num_cols as usize) {
                w.write_u8(entry, 4);
            }
        }
    }

    fn encode_window(w: &mut BitWriter, win: &Hdr10PlusWindow) {
        w.write_u16(win.upper_left_corner_x, 16);
        w.write_u16(win.upper_left_corner_y, 16);
        w.write_u16(win.lower_right_corner_x, 16);
        w.write_u16(win.lower_right_corner_y, 16);
        w.write_u16(win.center_of_ellipse_x, 16);
        w.write_u16(win.center_of_ellipse_y, 16);
        w.write_u8(win.rotation_angle, 8);
        w.write_u16(win.semimajor_axis_internal_ellipse, 16);
        w.write_u16(win.semimajor_axis_external_ellipse, 16);
        w.write_u16(win.semiminor_axis_external_ellipse, 16);
        w.write_bool(win.overlap_process_option);
        for &v in win.maxscl.iter() {
            w.write_u32(v, 17);
        }
        w.write_u32(win.average_maxrgb, 17);
        w.write_u8(win.distribution_maxrgb.count, 4);
        for i in 0..win.distribution_maxrgb.count as usize {
            w.write_u8(win.distribution_maxrgb.percentages[i], 7);
            w.write_u32(win.distribution_maxrgb.percentiles[i], 17);
        }
        w.write_u16(win.fraction_bright_pixels, 10);
    }

    fn decode_window(r: &mut BitReader<'_>) -> Result<Hdr10PlusWindow, DecodeError> {
        let upper_left_corner_x = r.read_u16(16)?;
        let upper_left_corner_y = r.read_u16(16)?;
        let lower_right_corner_x = r.read_u16(16)?;
        let lower_right_corner_y = r.read_u16(16)?;
        let center_of_ellipse_x = r.read_u16(16)?;
        let center_of_ellipse_y = r.read_u16(16)?;
        let rotation_angle = r.read_u8(8)?;
        let semimajor_axis_internal_ellipse = r.read_u16(16)?;
        let semimajor_axis_external_ellipse = r.read_u16(16)?;
        let semiminor_axis_external_ellipse = r.read_u16(16)?;
        let overlap_process_option = r.read_bool()?;
        let maxscl = [r.read_u32(17)?, r.read_u32(17)?, r.read_u32(17)?];
        let average_maxrgb = r.read_u32(17)?;

        let num_percentiles = r.read_u8(4)?;
        let mut distribution_maxrgb = DistributionMaxrgb {
            count: num_percentiles,
            ..Default::default()
        };
        for i in 0..num_percentiles as usize {
            distribution_maxrgb.percentages[i] = r.read_u8(7)?;
            distribution_maxrgb.percentiles[i] = r.read_u32(17)?;
        }

        let fraction_bright_pixels = r.read_u16(10)?;

        Ok(Hdr10PlusWindow {
            upper_left_corner_x,
            upper_left_corner_y,
            lower_right_corner_x,
            lower_right_corner_y,
            center_of_ellipse_x,
            center_of_ellipse_y,
            rotation_angle,
            semimajor_axis_internal_ellipse,
            semimajor_axis_external_ellipse,
            semiminor_axis_external_ellipse,
            overlap_process_option,
            maxscl,
            average_maxrgb,
            distribution_maxrgb,
            fraction_bright_pixels,
            // tone_mapping fields are filled in the second pass in decode()
            tone_mapping_flag: false,
            knee_point: None,
            bezier_curve_anchors: BezierAnchors::default(),
        })
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl SlHdrMetadata {
    /// Parse an SL-HDR metadata payload (ETSI TS 103 433-1 Table A.1).
    ///
    /// `push_warning` is called for each non-fatal anomaly encountered
    /// (unrecognised `sl_hdr_payload_mode` values).
    ///
    /// # Notes
    ///
    /// `GamutMappingEnabledFlag` is not defined within the SL-HDR payload
    /// itself — it comes from an outer HEVC context unavailable in a standalone
    /// HDMI payload. The gamut-mapping block is therefore always skipped.
    /// `gamut_mapping_params()` is likewise undefined in the spec excerpt and
    /// is not parsed.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::MalformedPayload`] if the payload is too short.
    pub fn decode(
        payload: &[u8],
        push_warning: &mut impl FnMut(DynamicHdrWarning),
    ) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(payload);

        let itu_t_t35_country_code = r.read_u8(8)?;
        let terminal_provider_code = r.read_u16(16)?;
        let terminal_provider_oriented_code_message_idc = r.read_u8(8)?;
        let sl_hdr_mode_value_minus1 = r.read_u8(4)?;
        let sl_hdr_spec_major_version_idc = r.read_u8(4)?;
        let sl_hdr_spec_minor_version_idc = r.read_u8(7)?;
        let sl_hdr_cancel_flag = r.read_bool()?;

        let body = if sl_hdr_cancel_flag {
            None
        } else {
            Some(Self::decode_body(&mut r, push_warning)?)
        };

        Ok(SlHdrMetadata {
            itu_t_t35_country_code,
            terminal_provider_code,
            terminal_provider_oriented_code_message_idc,
            sl_hdr_mode_value_minus1,
            sl_hdr_spec_major_version_idc,
            sl_hdr_spec_minor_version_idc,
            sl_hdr_cancel_flag,
            body,
        })
    }

    fn decode_body(
        r: &mut BitReader<'_>,
        push_warning: &mut impl FnMut(DynamicHdrWarning),
    ) -> Result<SlHdrBody, DecodeError> {
        let sl_hdr_persistence_flag = r.read_bool()?;
        let original_picture_info_present_flag = r.read_bool()?;
        let target_picture_info_present_flag = r.read_bool()?;
        let src_mdcv_info_present_flag = r.read_bool()?;
        let sl_hdr_extension_present_flag = r.read_bool()?;
        let sl_hdr_payload_mode = r.read_u8(3)?;

        let original_picture_info = if original_picture_info_present_flag {
            Some(SlHdrPictureInfo {
                primaries: r.read_u8(8)?,
                max_luminance: r.read_u16(16)?,
                min_luminance: r.read_u16(16)?,
            })
        } else {
            None
        };

        let target_picture_info = if target_picture_info_present_flag {
            Some(SlHdrPictureInfo {
                primaries: r.read_u8(8)?,
                max_luminance: r.read_u16(16)?,
                min_luminance: r.read_u16(16)?,
            })
        } else {
            None
        };

        let src_mdcv_info = if src_mdcv_info_present_flag {
            let mut primaries = [[0u16; 2]; 3];
            for component in primaries.iter_mut() {
                component[0] = r.read_u16(16)?; // x
                component[1] = r.read_u16(16)?; // y
            }
            Some(SlHdrMdcvInfo {
                primaries,
                ref_white_x: r.read_u16(16)?,
                ref_white_y: r.read_u16(16)?,
                max_mastering_luminance: r.read_u16(16)?,
                min_mastering_luminance: r.read_u16(16)?,
            })
        } else {
            None
        };

        let mut matrix_coefficient_values = [0u16; 4];
        for v in matrix_coefficient_values.iter_mut() {
            *v = r.read_u16(16)?;
        }
        let mut chroma_to_luma_injection = [0u16; 2];
        for v in chroma_to_luma_injection.iter_mut() {
            *v = r.read_u16(16)?;
        }
        let mut k_coefficient_values = [0u8; 3];
        for v in k_coefficient_values.iter_mut() {
            *v = r.read_u8(8)?;
        }

        let payload = match sl_hdr_payload_mode {
            0 => SlHdrPayload::Mode0(Self::decode_mode0(r)?),
            1 => SlHdrPayload::Mode1(alloc::boxed::Box::new(Self::decode_mode1(r)?)),
            other => {
                push_warning(DynamicHdrWarning::UnknownEnumValue {
                    field: "sl_hdr_payload_mode",
                    raw: other,
                });
                SlHdrPayload::Unknown(other)
            }
        };

        // GamutMappingEnabledFlag is not present in a standalone HDMI payload;
        // skip the gamut-mapping block entirely.

        let extension = if sl_hdr_extension_present_flag {
            let extension_6bits = r.read_u8(6)?;
            let length = r.read_u16(10)? as usize;
            let mut data = alloc::vec::Vec::with_capacity(length);
            for _ in 0..length {
                data.push(r.read_u8(8)?);
            }
            Some(SlHdrExtension {
                extension_6bits,
                data,
            })
        } else {
            None
        };

        Ok(SlHdrBody {
            sl_hdr_persistence_flag,
            sl_hdr_payload_mode,
            original_picture_info,
            target_picture_info,
            src_mdcv_info,
            matrix_coefficient_values,
            chroma_to_luma_injection,
            k_coefficient_values,
            payload,
            extension,
        })
    }

    fn decode_mode0(r: &mut BitReader<'_>) -> Result<SlHdrMode0, DecodeError> {
        let tone_mapping_input_signal_black_level_offset = r.read_u8(8)?;
        let tone_mapping_input_signal_white_level_offset = r.read_u8(8)?;
        let shadow_gain_control = r.read_u8(8)?;
        let highlight_gain_control = r.read_u8(8)?;
        let mid_tone_width_adjustment_factor = r.read_u8(8)?;

        let ftm_count = r.read_u8(4)?;
        let sg_count = r.read_u8(4)?;
        let mut tone_mapping_output_fine_tuning = SlHdrTable15 {
            count: ftm_count,
            ..Default::default()
        };
        for i in 0..ftm_count as usize {
            tone_mapping_output_fine_tuning.x[i] = r.read_u8(8)?;
            tone_mapping_output_fine_tuning.y[i] = r.read_u8(8)?;
        }
        let mut saturation_gain = SlHdrTable15 {
            count: sg_count,
            ..Default::default()
        };
        for i in 0..sg_count as usize {
            saturation_gain.x[i] = r.read_u8(8)?;
            saturation_gain.y[i] = r.read_u8(8)?;
        }

        Ok(SlHdrMode0 {
            tone_mapping_input_signal_black_level_offset,
            tone_mapping_input_signal_white_level_offset,
            shadow_gain_control,
            highlight_gain_control,
            mid_tone_width_adjustment_factor,
            tone_mapping_output_fine_tuning,
            saturation_gain,
        })
    }

    fn decode_mode1(r: &mut BitReader<'_>) -> Result<SlHdrMode1, DecodeError> {
        let lm_uniform_sampling_flag = r.read_bool()?;
        let lm_count = r.read_u8(7)?;
        let mut luminance_mapping = SlHdrTable127 {
            count: lm_count,
            ..Default::default()
        };
        for i in 0..lm_count as usize {
            if !lm_uniform_sampling_flag {
                luminance_mapping.x[i] = r.read_u16(16)?;
            }
            luminance_mapping.y[i] = r.read_u16(16)?;
        }

        let cc_uniform_sampling_flag = r.read_bool()?;
        let cc_count = r.read_u8(7)?;
        let mut colour_correction = SlHdrTable127 {
            count: cc_count,
            ..Default::default()
        };
        for i in 0..cc_count as usize {
            if !cc_uniform_sampling_flag {
                colour_correction.x[i] = r.read_u16(16)?;
            }
            colour_correction.y[i] = r.read_u16(16)?;
        }

        Ok(SlHdrMode1 {
            lm_uniform_sampling_flag,
            luminance_mapping,
            cc_uniform_sampling_flag,
            colour_correction,
        })
    }

    /// Serialize this metadata to a byte buffer using `BitWriter`.
    ///
    /// Returns `(buf, len)` — the populated prefix of `buf`.
    pub(crate) fn encode(&self) -> ([u8; MAX_DYNAMIC_HDR_PAYLOAD], usize) {
        let mut w = BitWriter::new();

        w.write_u8(self.itu_t_t35_country_code, 8);
        w.write_u16(self.terminal_provider_code, 16);
        w.write_u8(self.terminal_provider_oriented_code_message_idc, 8);
        w.write_u8(self.sl_hdr_mode_value_minus1, 4);
        w.write_u8(self.sl_hdr_spec_major_version_idc, 4);
        w.write_u8(self.sl_hdr_spec_minor_version_idc, 7);
        w.write_bool(self.sl_hdr_cancel_flag);

        if let Some(ref body) = self.body {
            Self::encode_body(&mut w, body);
        }

        w.finish()
    }

    fn encode_body(w: &mut BitWriter, body: &SlHdrBody) {
        w.write_bool(body.sl_hdr_persistence_flag);
        w.write_bool(body.original_picture_info.is_some());
        w.write_bool(body.target_picture_info.is_some());
        w.write_bool(body.src_mdcv_info.is_some());
        #[cfg(any(feature = "alloc", feature = "std"))]
        w.write_bool(body.extension.is_some());
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        w.write_bool(false); // no extension storage in bare no_std
        w.write_u8(body.sl_hdr_payload_mode, 3);

        if let Some(ref info) = body.original_picture_info {
            w.write_u8(info.primaries, 8);
            w.write_u16(info.max_luminance, 16);
            w.write_u16(info.min_luminance, 16);
        }
        if let Some(ref info) = body.target_picture_info {
            w.write_u8(info.primaries, 8);
            w.write_u16(info.max_luminance, 16);
            w.write_u16(info.min_luminance, 16);
        }
        if let Some(ref mdcv) = body.src_mdcv_info {
            for component in mdcv.primaries.iter() {
                w.write_u16(component[0], 16);
                w.write_u16(component[1], 16);
            }
            w.write_u16(mdcv.ref_white_x, 16);
            w.write_u16(mdcv.ref_white_y, 16);
            w.write_u16(mdcv.max_mastering_luminance, 16);
            w.write_u16(mdcv.min_mastering_luminance, 16);
        }

        for &v in body.matrix_coefficient_values.iter() {
            w.write_u16(v, 16);
        }
        for &v in body.chroma_to_luma_injection.iter() {
            w.write_u16(v, 16);
        }
        for &v in body.k_coefficient_values.iter() {
            w.write_u8(v, 8);
        }

        match &body.payload {
            SlHdrPayload::Mode0(m) => Self::encode_mode0(w, m),
            #[cfg(any(feature = "alloc", feature = "std"))]
            SlHdrPayload::Mode1(m) => Self::encode_mode1(w, m),
            SlHdrPayload::Unknown(_) => {} // no bits to write for unknown mode
        }

        // GamutMappingEnabledFlag is always treated as false; no gamut block written.

        #[cfg(any(feature = "alloc", feature = "std"))]
        if let Some(ref ext) = body.extension {
            w.write_u8(ext.extension_6bits, 6);
            w.write_u16(ext.data.len() as u16, 10);
            for &byte in ext.data.iter() {
                w.write_u8(byte, 8);
            }
        }
    }

    fn encode_mode0(w: &mut BitWriter, m: &SlHdrMode0) {
        w.write_u8(m.tone_mapping_input_signal_black_level_offset, 8);
        w.write_u8(m.tone_mapping_input_signal_white_level_offset, 8);
        w.write_u8(m.shadow_gain_control, 8);
        w.write_u8(m.highlight_gain_control, 8);
        w.write_u8(m.mid_tone_width_adjustment_factor, 8);
        w.write_u8(m.tone_mapping_output_fine_tuning.count, 4);
        w.write_u8(m.saturation_gain.count, 4);
        for i in 0..m.tone_mapping_output_fine_tuning.count as usize {
            w.write_u8(m.tone_mapping_output_fine_tuning.x[i], 8);
            w.write_u8(m.tone_mapping_output_fine_tuning.y[i], 8);
        }
        for i in 0..m.saturation_gain.count as usize {
            w.write_u8(m.saturation_gain.x[i], 8);
            w.write_u8(m.saturation_gain.y[i], 8);
        }
    }

    fn encode_mode1(w: &mut BitWriter, m: &SlHdrMode1) {
        w.write_bool(m.lm_uniform_sampling_flag);
        w.write_u8(m.luminance_mapping.count, 7);
        for i in 0..m.luminance_mapping.count as usize {
            if !m.lm_uniform_sampling_flag {
                w.write_u16(m.luminance_mapping.x[i], 16);
            }
            w.write_u16(m.luminance_mapping.y[i], 16);
        }
        w.write_bool(m.cc_uniform_sampling_flag);
        w.write_u8(m.colour_correction.count, 7);
        for i in 0..m.colour_correction.count as usize {
            if !m.cc_uniform_sampling_flag {
                w.write_u16(m.colour_correction.x[i], 16);
            }
            w.write_u16(m.colour_correction.y[i], 16);
        }
    }
}

/// Iterator that yields 31-byte wire packets for a [`DynamicHdrInfoFrame`].
///
/// Produced by [`DynamicHdrInfoFrame::into_packets`]. Yields one packet per
/// 23-byte chunk of the serialized metadata payload; the final packet carries
/// any remaining bytes (fewer than 23).
pub struct DynamicHdrIter {
    format_id: u8,
    total_bytes: u16,
    offset: usize,
    seq_num: u8,
    #[cfg(any(feature = "alloc", feature = "std"))]
    payload: alloc::vec::Vec<u8>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    /// Length of valid bytes in `payload` (bare `no_std` builds only).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload_len: usize,
}

impl Iterator for DynamicHdrIter {
    type Item = [u8; 31];

    fn next(&mut self) -> Option<[u8; 31]> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        let payload_len = self.payload.len();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        let payload_len = self.payload_len;

        if self.offset >= payload_len {
            return None;
        }

        let chunk_len = (payload_len - self.offset).min(23);

        // Build the 30 non-checksum bytes: [type, version, length, PB0..PB26].
        // Byte layout (offsets in the final 31-byte packet):
        //   0: type_code=0x20, 1: version=0x01, 2: length=4+chunk_len
        //   3: checksum (filled below), 4: seq_num, 5–6: total_bytes LE,
        //   7: format_id, 8..8+chunk_len: chunk data
        let mut hp = [0u8; 30];
        hp[0] = 0x20; // Dynamic HDR type code
        hp[1] = 0x01; // version
        hp[2] = (4 + chunk_len) as u8;
        hp[3] = self.seq_num;
        let tb = self.total_bytes.to_le_bytes();
        hp[4] = tb[0];
        hp[5] = tb[1];
        hp[6] = self.format_id;
        hp[7..7 + chunk_len].copy_from_slice(&self.payload[self.offset..self.offset + chunk_len]);

        let checksum = crate::checksum::compute_checksum(&hp);

        let mut packet = [0u8; 31];
        packet[..3].copy_from_slice(&hp[..3]);
        packet[3] = checksum;
        packet[4..].copy_from_slice(&hp[3..]);

        self.offset += chunk_len;
        self.seq_num += 1;

        Some(packet)
    }
}

impl IntoPackets for DynamicHdrInfoFrame {
    type Iter = DynamicHdrIter;
    type Warning = DynamicHdrWarning;

    fn into_packets(self) -> Decoded<DynamicHdrIter, DynamicHdrWarning> {
        match self {
            DynamicHdrInfoFrame::Unknown {
                format_id,
                #[cfg(any(feature = "alloc", feature = "std"))]
                payload,
            } => {
                #[cfg(any(feature = "alloc", feature = "std"))]
                let total_bytes = payload.len() as u16;
                #[cfg(not(any(feature = "alloc", feature = "std")))]
                let total_bytes: u16 = 0;

                Decoded::new(DynamicHdrIter {
                    format_id,
                    total_bytes,
                    offset: 0,
                    seq_num: 0,
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    payload,
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload: [0u8; MAX_DYNAMIC_HDR_PAYLOAD],
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload_len: 0,
                })
            }
            #[cfg(any(feature = "alloc", feature = "std"))]
            DynamicHdrInfoFrame::Hdr10Plus(meta) => {
                let (buf, len) = meta.encode();
                let payload = buf[..len].to_vec();
                let total_bytes = len as u16;
                Decoded::new(DynamicHdrIter {
                    format_id: 0x04,
                    total_bytes,
                    offset: 0,
                    seq_num: 0,
                    payload,
                })
            }
            #[cfg(any(feature = "alloc", feature = "std"))]
            DynamicHdrInfoFrame::SlHdr(meta) => {
                let (buf, len) = meta.encode();
                let payload = buf[..len].to_vec();
                let total_bytes = len as u16;
                Decoded::new(DynamicHdrIter {
                    format_id: 0x02,
                    total_bytes,
                    offset: 0,
                    seq_num: 0,
                    payload,
                })
            }
        }
    }
}

/// A single packet's worth of Dynamic HDR metadata, as returned by the
/// top-level [`decode`](crate::decode) function.
///
/// A full [`DynamicHdrInfoFrame`] cannot be assembled from a single wire
/// packet. The top-level decode path therefore returns this fragment type,
/// which exposes the fields the caller needs to accumulate a complete sequence.
/// Once all packets in the sequence have been collected, pass them to
/// `DynamicHdrInfoFrame::decode_sequence` to assemble the full frame.
///
/// # Wire layout
///
/// ```text
/// Byte 4 (PB0):    seq_num    — packet index in sequence (0-based)
/// Bytes 5–6 (PB1–2): total_bytes — total metadata byte count (little-endian u16)
/// Byte 7 (PB3):    format_id  — metadata format identifier
/// Bytes 8–30 (PB4–26): chunk  — up to 23 metadata bytes
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DynamicHdrFragment {
    /// Zero-indexed position of this packet in the sequence.
    pub seq_num: u8,
    /// Total metadata byte count declared in the packet header.
    ///
    /// The sequence is complete when the sum of `chunk_len` values across all
    /// received fragments reaches this value.
    pub total_bytes: u16,
    /// Identifies the metadata format (HDR10+, SL-HDR, etc.).
    ///
    /// Unrecognised format identifiers are preserved here; `Unknown` at the
    /// `InfoFramePacket` level is a type-code catch-all, not a format catch-all.
    pub format_id: u8,
    /// The metadata bytes carried by this packet.
    ///
    /// Only `chunk[..chunk_len as usize]` contains meaningful data. The final
    /// packet in a sequence may carry fewer than 23 bytes.
    pub chunk: [u8; 23],
    /// Number of valid bytes in [`chunk`](DynamicHdrFragment::chunk).
    ///
    /// Always ≤ 23.
    pub chunk_len: u8,
}

impl DynamicHdrFragment {
    /// Decode a single Dynamic HDR InfoFrame packet into a fragment.
    ///
    /// Each Dynamic HDR packet carries a sequence index, the total metadata
    /// byte count, a format identifier, and up to 23 bytes of metadata. The
    /// caller is responsible for collecting fragments until the sequence is
    /// complete, then passing the full packet sequence to
    /// `DynamicHdrInfoFrame::decode_sequence`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`]
    pub fn decode(
        packet: &[u8; 31],
    ) -> Result<Decoded<DynamicHdrFragment, DynamicHdrWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated { claimed: length });
        }

        let mut decoded = Decoded::new(DynamicHdrFragment {
            seq_num: 0,
            total_bytes: 0,
            format_id: 0,
            chunk: [0u8; 23],
            chunk_len: 0,
        });

        // Checksum verification.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        decoded.value.seq_num = packet[4];
        decoded.value.total_bytes = u16::from_le_bytes([packet[5], packet[6]]);
        decoded.value.format_id = packet[7];

        // chunk_len = payload bytes after the 4-byte overhead, capped at 23.
        let chunk_len = length.saturating_sub(4).min(23);
        decoded.value.chunk_len = chunk_len;
        decoded.value.chunk[..chunk_len as usize]
            .copy_from_slice(&packet[8..8 + chunk_len as usize]);

        Ok(decoded)
    }
}

#[cfg(test)]
mod tests;
