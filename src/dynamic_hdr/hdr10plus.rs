use super::{BitReader, BitWriter, MAX_DYNAMIC_HDR_PAYLOAD};
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;
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
