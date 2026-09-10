use super::{BitReader, BitWriter, MAX_DYNAMIC_HDR_PAYLOAD};
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;
// ---------------------------------------------------------------------------
// SL-HDR metadata types (ETSI TS 103 433-1 Table A.1)
// ---------------------------------------------------------------------------

/// SL-HDR dynamic metadata (ETSI TS 103 433-1 Table A.1, format identifier
/// `0x02`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    ///
    /// In bare `no_std` builds this field is absent and the extension is not
    /// retained — encoding a decoded extension-carrying stream is lossy.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub extension: Option<SlHdrExtension>,
}

/// Picture colour and luminance info (original or target picture).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
///
/// `Mode1` carries two 127-entry `u16` tables (approximately 1,000 bytes
/// total). Boxing would require `alloc`, so the data is stored inline in all
/// build configurations. Callers who decode mode-1 streams on stack-limited
/// targets should store the enclosing [`SlHdrMetadata`] in a `static` or
/// equivalent.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlHdrPayload {
    /// `sl_hdr_payload_mode == 0`: tone-mapping tables.
    Mode0(SlHdrMode0),
    /// `sl_hdr_payload_mode == 1`: luminance/colour mapping tables.
    Mode1(SlHdrMode1),
    /// Any other `sl_hdr_payload_mode` value; the raw mode byte is preserved.
    ///
    /// Encoding this variant is lossy: only the `sl_hdr_payload_mode` field is
    /// written; the payload body bytes are not retained and cannot be re-emitted.
    Unknown(u8),
}

/// Tone-mapping payload (`sl_hdr_payload_mode == 0`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlHdrExtension {
    /// `sl_hdr_extension_6bits` (6 bits).
    pub extension_6bits: u8,
    /// Raw `sl_hdr_extension_data_byte` bytes.
    pub data: alloc::vec::Vec<u8>,
}

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
            1 => SlHdrPayload::Mode1(Self::decode_mode1(r)?),
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

        #[cfg(any(feature = "alloc", feature = "std"))]
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
        // In bare no_std builds extension data is consumed and discarded; it is
        // optional and cannot be retained without heap allocation.
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        if sl_hdr_extension_present_flag {
            let _ext_6bits = r.read_u8(6)?;
            let length = r.read_u16(10)? as usize;
            for _ in 0..length {
                r.read_u8(8)?;
            }
        }

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
            #[cfg(any(feature = "alloc", feature = "std"))]
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
            SlHdrPayload::Mode1(m) => Self::encode_mode1(w, m),
            SlHdrPayload::Unknown(_) => {} // no bits to write for unknown mode — encode is lossy
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

// ---------------------------------------------------------------------------
// Manual serde impl for SlHdrTable127
//
// serde's derive supports arrays only up to 32 elements; `[u16; 127]` requires
// a hand-written impl.  The format serialises only the `count` valid entries
// (not all 127 slots), which is both smaller and more meaningful.
// ---------------------------------------------------------------------------
#[cfg(feature = "serde")]
const _: () = {
    use serde::de::{self, MapAccess, SeqAccess, Visitor};
    use serde::ser::SerializeStruct;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for SlHdrTable127 {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let n = self.count as usize;
            let mut st = s.serialize_struct("SlHdrTable127", 3)?;
            st.serialize_field("count", &self.count)?;
            st.serialize_field("x", &&self.x[..n])?;
            st.serialize_field("y", &&self.y[..n])?;
            st.end()
        }
    }

    impl<'de> Deserialize<'de> for SlHdrTable127 {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(serde::Deserialize)]
            #[serde(field_identifier, rename_all = "lowercase")]
            enum Field {
                Count,
                X,
                Y,
            }

            /// Deserialise a sequence of up to 127 `u16` values into a
            /// fixed-size `[u16; 127]` array without heap allocation.
            struct Arr127(pub [u16; 127]);
            impl<'de> Deserialize<'de> for Arr127 {
                fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                    struct Seq127Visitor;
                    impl<'de> Visitor<'de> for Seq127Visitor {
                        type Value = Arr127;
                        fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                            f.write_str("a sequence of up to 127 u16 values")
                        }
                        fn visit_seq<A: SeqAccess<'de>>(
                            self,
                            mut seq: A,
                        ) -> Result<Arr127, A::Error> {
                            let mut arr = [0u16; 127];
                            let mut i = 0usize;
                            while let Some(v) = seq.next_element()? {
                                if i >= 127 {
                                    return Err(de::Error::invalid_length(
                                        i + 1,
                                        &"at most 127 elements",
                                    ));
                                }
                                arr[i] = v;
                                i += 1;
                            }
                            Ok(Arr127(arr))
                        }
                    }
                    d.deserialize_seq(Seq127Visitor)
                }
            }

            struct T127Visitor;
            impl<'de> Visitor<'de> for T127Visitor {
                type Value = SlHdrTable127;
                fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                    f.write_str("struct SlHdrTable127")
                }
                fn visit_map<V: MapAccess<'de>>(
                    self,
                    mut map: V,
                ) -> Result<SlHdrTable127, V::Error> {
                    let mut count: Option<u8> = None;
                    let mut x: Option<Arr127> = None;
                    let mut y: Option<Arr127> = None;
                    while let Some(key) = map.next_key()? {
                        match key {
                            Field::Count => {
                                count = Some(map.next_value()?);
                            }
                            Field::X => {
                                x = Some(map.next_value()?);
                            }
                            Field::Y => {
                                y = Some(map.next_value()?);
                            }
                        }
                    }
                    let count = count.ok_or_else(|| de::Error::missing_field("count"))?;
                    let x = x.ok_or_else(|| de::Error::missing_field("x"))?;
                    let y = y.ok_or_else(|| de::Error::missing_field("y"))?;
                    Ok(SlHdrTable127 {
                        count,
                        x: x.0,
                        y: y.0,
                    })
                }
            }

            const FIELDS: &[&str] = &["count", "x", "y"];
            d.deserialize_struct("SlHdrTable127", FIELDS, T127Visitor)
        }
    }
};
