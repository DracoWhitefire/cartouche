use display_types::ColorFormat;

use crate::decoded::Decoded;
use crate::encode::{IntoPackets, SinglePacketIter};
use crate::error::DecodeError;
use crate::warn::AviWarning;

/// Bar data presence flags (B field, PB1 bits 3–2).
///
/// Indicates which bar data fields in PB6–PB13 carry valid measurements.
/// Fields not indicated as present contain undefined bytes on the wire and
/// should not be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BarInfo {
    /// No bar data present (B = 0b00).
    NotPresent,
    /// Left and right bar pixel numbers are valid (B = 0b01).
    VerticalBarsPresent,
    /// Top and bottom bar line numbers are valid (B = 0b10).
    HorizontalBarsPresent,
    /// All four bar measurements are valid (B = 0b11).
    BothPresent,
}

/// Scan information (S field, PB1 bits 1–0).
///
/// Describes how the source expects the sink to handle overscan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanInfo {
    /// No scan information available (S = 0).
    NoData,
    /// Content is composed for a display that overscans (S = 1).
    Overscanned,
    /// Content is composed for a display that underscans (S = 2).
    Underscanned,
}

/// Primary colorimetry (C field, PB2 bits 7–6).
///
/// When set to [`Extended`](Colorimetry::Extended), the
/// [`extended_colorimetry`](AviInfoFrame::extended_colorimetry) field carries the
/// precise colorimetry value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Colorimetry {
    /// No colorimetry data (C = 0b00).
    NoData,
    /// SMPTE 170M / ITU-R BT.601 (C = 0b01).
    Bt601,
    /// ITU-R BT.709 (C = 0b10).
    Bt709,
    /// Extended colorimetry — refer to the EC field (C = 0b11).
    Extended,
}

/// Extended colorimetry (EC field, PB3 bits 6–4).
///
/// Only meaningful when [`colorimetry`](AviInfoFrame::colorimetry) is
/// [`Colorimetry::Extended`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtendedColorimetry {
    /// xvYCC 601 (EC = 0).
    XvYCC601,
    /// xvYCC 709 (EC = 1).
    XvYCC709,
    /// sYCC 601 (EC = 2).
    SyCC601,
    /// opYCC 601 (EC = 3).
    OpYCC601,
    /// opRGB (EC = 4).
    OpRgb,
    /// BT.2020 constant luminance YCbCr (EC = 5).
    Bt2020cYCC,
    /// BT.2020 non-constant luminance YCbCr (EC = 6).
    Bt2020YCC,
    /// Additional Colorimetry Extension present (EC = 7).
    ///
    /// Defined in CTA-861-H for AVI InfoFrame version 3 and later.
    /// Version 2 (HDMI 2.1) sinks may not recognise this value.
    AdditionalColorimetryExtension,
}

/// Picture aspect ratio (M field, PB2 bits 5–4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PictureAspectRatio {
    /// No aspect ratio data (M = 0b00).
    NoData,
    /// 4:3 picture aspect ratio (M = 0b01).
    FourByThree,
    /// 16:9 picture aspect ratio (M = 0b10).
    SixteenByNine,
}

/// RGB quantization range (Q field, PB3 bits 3–2).
///
/// Only meaningful when [`color_format`](AviInfoFrame::color_format) is
/// [`ColorFormat::Rgb444`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RgbQuantization {
    /// Default quantization range for the video format (Q = 0b00).
    Default,
    /// Limited range: levels 16–235 for Y, 16–240 for Cb/Cr (Q = 0b01).
    LimitedRange,
    /// Full range: levels 0–255 (Q = 0b10).
    FullRange,
}

/// YCC quantization range (YQ field, PB5 bits 7–6).
///
/// Only meaningful when [`color_format`](AviInfoFrame::color_format) is a
/// YCbCr variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum YccQuantization {
    /// Limited YCC quantization range (YQ = 0b00).
    LimitedRange,
    /// Full YCC quantization range (YQ = 0b01).
    FullRange,
}

/// Non-uniform picture scaling (SC field, PB3 bits 1–0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NonUniformScaling {
    /// No known non-uniform scaling (SC = 0b00).
    None,
    /// Picture has been scaled horizontally (SC = 0b01).
    Horizontal,
    /// Picture has been scaled vertically (SC = 0b10).
    Vertical,
    /// Picture has been scaled both horizontally and vertically (SC = 0b11).
    Both,
}

/// IT content type (CN field, PB5 bits 5–4).
///
/// Only meaningful when [`it_content`](AviInfoFrame::it_content) is `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ItContentType {
    /// Graphics content (CN = 0b00).
    Graphics,
    /// Photo content (CN = 0b01).
    Photo,
    /// Cinema content (CN = 0b10).
    Cinema,
    /// Game content (CN = 0b11).
    Game,
}

/// An AVI InfoFrame (CTA-861, type code 0x82).
///
/// Carries the color space, colorimetry, quantization range, aspect ratio,
/// active format, VIC, pixel repetition count, and optional bar data.
/// Required for the sink to configure its display pipeline correctly.
///
/// # Bit layout reference (CTA-861-H §6.4)
///
/// - PB1: `color_format`\[7:5\], `active_format_present`\[4\], `bar_info`\[3:2\], `scan_info`\[1:0\]
/// - PB2: `colorimetry`\[7:6\], `picture_aspect_ratio`\[5:4\], `active_format_aspect_ratio`\[3:0\]
/// - PB3: `it_content`\[7\], `extended_colorimetry`\[6:4\], `rgb_quantization`\[3:2\],
///   `non_uniform_scaling`\[1:0\]
/// - PB4: reserved\[7\], `vic`\[6:0\]
/// - PB5: `ycc_quantization`\[7:6\], `it_content_type`\[5:4\], `pixel_repetition`\[3:0\]
/// - PB6–PB7: `top_bar` (little-endian)
/// - PB8–PB9: `bottom_bar` (little-endian)
/// - PB10–PB11: `left_bar` (little-endian)
/// - PB12–PB13: `right_bar` (little-endian)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AviInfoFrame {
    // ---- PB1 ----
    /// Video color format (Y\[2:0\]).
    pub color_format: ColorFormat,
    /// Active format description is present in [`active_format_aspect_ratio`](Self::active_format_aspect_ratio).
    pub active_format_present: bool,
    /// Which bar measurements (if any) are valid.
    pub bar_info: BarInfo,
    /// Overscan/underscan indication.
    pub scan_info: ScanInfo,

    // ---- PB2 ----
    /// Primary colorimetry standard.
    pub colorimetry: Colorimetry,
    /// Extended colorimetry standard; valid only when `colorimetry` is [`Colorimetry::Extended`].
    pub extended_colorimetry: ExtendedColorimetry,
    /// Coded picture aspect ratio.
    pub picture_aspect_ratio: PictureAspectRatio,
    /// Active Format Description (AFD) code, R\[3:0\]. See CTA-861 §6.4.
    ///
    /// Only meaningful when [`active_format_present`](Self::active_format_present) is `true`.
    pub active_format_aspect_ratio: u8,

    // ---- PB3 ----
    /// IT (Information Technology) content flag.
    pub it_content: bool,
    /// RGB quantization range; valid only when `color_format` is [`ColorFormat::Rgb444`].
    pub rgb_quantization: RgbQuantization,
    /// Non-uniform picture scaling applied by the source.
    pub non_uniform_scaling: NonUniformScaling,

    // ---- PB4 ----
    /// Video Identification Code (VIC), 0–127.
    pub vic: u8,

    // ---- PB5 ----
    /// YCC quantization range; valid only when `color_format` is a YCbCr variant.
    pub ycc_quantization: YccQuantization,
    /// IT content type; valid only when [`it_content`](Self::it_content) is `true`.
    pub it_content_type: ItContentType,
    /// Pixel repetition factor. `0` = no repetition (sent once), `1` = sent twice, …, `9` = sent ten times.
    pub pixel_repetition: u8,

    // ---- PB6–PB13: bar data ----
    /// End of top bar (line number). Valid only when `bar_info` indicates horizontal bars.
    pub top_bar: u16,
    /// Start of bottom bar (line number). Valid only when `bar_info` indicates horizontal bars.
    pub bottom_bar: u16,
    /// End of left bar (pixel number). Valid only when `bar_info` indicates vertical bars.
    pub left_bar: u16,
    /// Start of right bar (pixel number). Valid only when `bar_info` indicates vertical bars.
    pub right_bar: u16,
}

impl AviInfoFrame {
    /// Decode an AVI InfoFrame from a 31-byte wire packet.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry any of:
    /// - [`AviWarning::ChecksumMismatch`]
    /// - [`AviWarning::ReservedFieldNonZero`]
    /// - [`AviWarning::UnknownEnumValue`]
    pub fn decode(packet: &[u8; 31]) -> Result<Decoded<AviInfoFrame, AviWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated {
                claimed: length,
                available: 27,
            });
        }

        let mut decoded = Decoded::new(AviInfoFrame {
            color_format: ColorFormat::Rgb444,
            active_format_present: false,
            bar_info: BarInfo::NotPresent,
            scan_info: ScanInfo::NoData,
            colorimetry: Colorimetry::NoData,
            extended_colorimetry: ExtendedColorimetry::XvYCC601,
            picture_aspect_ratio: PictureAspectRatio::NoData,
            active_format_aspect_ratio: 0,
            it_content: false,
            rgb_quantization: RgbQuantization::Default,
            non_uniform_scaling: NonUniformScaling::None,
            vic: 0,
            ycc_quantization: YccQuantization::LimitedRange,
            it_content_type: ItContentType::Graphics,
            pixel_repetition: 0,
            top_bar: 0,
            bottom_bar: 0,
            left_bar: 0,
            right_bar: 0,
        });

        // Checksum.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(AviWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        // PB1 (byte 4): Y2[7]|Y1[6]|Y0[5]|A0[4]|B1[3]|B0[2]|S1[1]|S0[0]
        let pb1 = packet[4];
        let y_raw = (pb1 >> 5) & 0x07;
        decoded.value.color_format = match y_raw {
            0 => ColorFormat::Rgb444,
            1 => ColorFormat::YCbCr422,
            2 => ColorFormat::YCbCr444,
            3 => ColorFormat::YCbCr420,
            _ => {
                decoded.push_warning(AviWarning::UnknownEnumValue {
                    field: "color_format",
                    raw: y_raw,
                });
                ColorFormat::Rgb444
            }
        };
        decoded.value.active_format_present = pb1 & 0x10 != 0;
        decoded.value.bar_info = match (pb1 >> 2) & 0x03 {
            0 => BarInfo::NotPresent,
            1 => BarInfo::VerticalBarsPresent,
            2 => BarInfo::HorizontalBarsPresent,
            _ => BarInfo::BothPresent,
        };
        let s_raw = pb1 & 0x03;
        decoded.value.scan_info = match s_raw {
            0 => ScanInfo::NoData,
            1 => ScanInfo::Overscanned,
            2 => ScanInfo::Underscanned,
            _ => {
                decoded.push_warning(AviWarning::UnknownEnumValue {
                    field: "scan_info",
                    raw: s_raw,
                });
                ScanInfo::NoData
            }
        };

        // PB2 (byte 5): C1[7]|C0[6]|M1[5]|M0[4]|R3[3]|R2[2]|R1[1]|R0[0]
        let pb2 = packet[5];
        decoded.value.colorimetry = match (pb2 >> 6) & 0x03 {
            0 => Colorimetry::NoData,
            1 => Colorimetry::Bt601,
            2 => Colorimetry::Bt709,
            _ => Colorimetry::Extended,
        };
        let m_raw = (pb2 >> 4) & 0x03;
        decoded.value.picture_aspect_ratio = match m_raw {
            0 => PictureAspectRatio::NoData,
            1 => PictureAspectRatio::FourByThree,
            2 => PictureAspectRatio::SixteenByNine,
            _ => {
                decoded.push_warning(AviWarning::UnknownEnumValue {
                    field: "picture_aspect_ratio",
                    raw: m_raw,
                });
                PictureAspectRatio::NoData
            }
        };
        decoded.value.active_format_aspect_ratio = pb2 & 0x0F;

        // PB3 (byte 6): IT[7]|EC2[6]|EC1[5]|EC0[4]|Q1[3]|Q0[2]|SC1[1]|SC0[0]
        let pb3 = packet[6];
        decoded.value.it_content = pb3 & 0x80 != 0;
        decoded.value.extended_colorimetry = match (pb3 >> 4) & 0x07 {
            0 => ExtendedColorimetry::XvYCC601,
            1 => ExtendedColorimetry::XvYCC709,
            2 => ExtendedColorimetry::SyCC601,
            3 => ExtendedColorimetry::OpYCC601,
            4 => ExtendedColorimetry::OpRgb,
            5 => ExtendedColorimetry::Bt2020cYCC,
            6 => ExtendedColorimetry::Bt2020YCC,
            _ => ExtendedColorimetry::AdditionalColorimetryExtension,
        };
        let q_raw = (pb3 >> 2) & 0x03;
        decoded.value.rgb_quantization = match q_raw {
            0 => RgbQuantization::Default,
            1 => RgbQuantization::LimitedRange,
            2 => RgbQuantization::FullRange,
            _ => {
                decoded.push_warning(AviWarning::UnknownEnumValue {
                    field: "rgb_quantization",
                    raw: q_raw,
                });
                RgbQuantization::Default
            }
        };
        decoded.value.non_uniform_scaling = match pb3 & 0x03 {
            0 => NonUniformScaling::None,
            1 => NonUniformScaling::Horizontal,
            2 => NonUniformScaling::Vertical,
            _ => NonUniformScaling::Both,
        };

        // PB4 (byte 7): rsvd[7]|VIC[6:0]
        let pb4 = packet[7];
        if pb4 & 0x80 != 0 {
            decoded.push_warning(AviWarning::ReservedFieldNonZero { byte: 7, bit: 7 });
        }
        decoded.value.vic = pb4 & 0x7F;

        // PB5 (byte 8): YQ1[7]|YQ0[6]|CN1[5]|CN0[4]|PR3[3]|PR2[2]|PR1[1]|PR0[0]
        let pb5 = packet[8];
        let yq_raw = (pb5 >> 6) & 0x03;
        decoded.value.ycc_quantization = match yq_raw {
            0 => YccQuantization::LimitedRange,
            1 => YccQuantization::FullRange,
            _ => {
                decoded.push_warning(AviWarning::UnknownEnumValue {
                    field: "ycc_quantization",
                    raw: yq_raw,
                });
                YccQuantization::LimitedRange
            }
        };
        decoded.value.it_content_type = match (pb5 >> 4) & 0x03 {
            0 => ItContentType::Graphics,
            1 => ItContentType::Photo,
            2 => ItContentType::Cinema,
            _ => ItContentType::Game,
        };
        decoded.value.pixel_repetition = pb5 & 0x0F;

        // PB6–PB13 (bytes 9–16): bar data.
        // Only present when length is sufficient; left at the zero default otherwise.
        if length >= 7 {
            decoded.value.top_bar = u16::from_le_bytes([packet[9], packet[10]]);
        }
        if length >= 9 {
            decoded.value.bottom_bar = u16::from_le_bytes([packet[11], packet[12]]);
        }
        if length >= 11 {
            decoded.value.left_bar = u16::from_le_bytes([packet[13], packet[14]]);
        }
        if length >= 13 {
            decoded.value.right_bar = u16::from_le_bytes([packet[15], packet[16]]);
        }

        Ok(decoded)
    }
}

impl IntoPackets for AviInfoFrame {
    type Iter = SinglePacketIter;

    fn into_packets(self) -> SinglePacketIter {
        let mut hp = [0u8; 30];
        hp[0] = 0x82; // type code (AVI)
        hp[1] = 0x02; // version
        hp[2] = 13; // length: PB1–PB13

        // PB1: Y2[7]|Y1[6]|Y0[5]|A0[4]|B1[3]|B0[2]|S1[1]|S0[0]
        let y_raw: u8 = match self.color_format {
            ColorFormat::Rgb444 => 0,
            ColorFormat::YCbCr422 => 1,
            ColorFormat::YCbCr444 => 2,
            ColorFormat::YCbCr420 => 3,
            _ => 0,
        };
        let b_raw: u8 = match self.bar_info {
            BarInfo::NotPresent => 0,
            BarInfo::VerticalBarsPresent => 1,
            BarInfo::HorizontalBarsPresent => 2,
            BarInfo::BothPresent => 3,
        };
        let s_raw: u8 = match self.scan_info {
            ScanInfo::NoData => 0,
            ScanInfo::Overscanned => 1,
            ScanInfo::Underscanned => 2,
        };
        hp[3] = (y_raw << 5) | ((self.active_format_present as u8) << 4) | (b_raw << 2) | s_raw;

        // PB2: C1[7]|C0[6]|M1[5]|M0[4]|R3[3]|R2[2]|R1[1]|R0[0]
        let c_raw: u8 = match self.colorimetry {
            Colorimetry::NoData => 0,
            Colorimetry::Bt601 => 1,
            Colorimetry::Bt709 => 2,
            Colorimetry::Extended => 3,
        };
        let m_raw: u8 = match self.picture_aspect_ratio {
            PictureAspectRatio::NoData => 0,
            PictureAspectRatio::FourByThree => 1,
            PictureAspectRatio::SixteenByNine => 2,
        };
        hp[4] = (c_raw << 6) | (m_raw << 4) | (self.active_format_aspect_ratio & 0x0F);

        // PB3: IT[7]|EC2[6]|EC1[5]|EC0[4]|Q1[3]|Q0[2]|SC1[1]|SC0[0]
        let ec_raw: u8 = match self.extended_colorimetry {
            ExtendedColorimetry::XvYCC601 => 0,
            ExtendedColorimetry::XvYCC709 => 1,
            ExtendedColorimetry::SyCC601 => 2,
            ExtendedColorimetry::OpYCC601 => 3,
            ExtendedColorimetry::OpRgb => 4,
            ExtendedColorimetry::Bt2020cYCC => 5,
            ExtendedColorimetry::Bt2020YCC => 6,
            ExtendedColorimetry::AdditionalColorimetryExtension => 7,
        };
        let q_raw: u8 = match self.rgb_quantization {
            RgbQuantization::Default => 0,
            RgbQuantization::LimitedRange => 1,
            RgbQuantization::FullRange => 2,
        };
        let sc_raw: u8 = match self.non_uniform_scaling {
            NonUniformScaling::None => 0,
            NonUniformScaling::Horizontal => 1,
            NonUniformScaling::Vertical => 2,
            NonUniformScaling::Both => 3,
        };
        hp[5] = ((self.it_content as u8) << 7) | (ec_raw << 4) | (q_raw << 2) | sc_raw;

        // PB4: rsvd[7]|VIC[6:0]
        hp[6] = self.vic & 0x7F;

        // PB5: YQ1[7]|YQ0[6]|CN1[5]|CN0[4]|PR3[3]|PR2[2]|PR1[1]|PR0[0]
        let yq_raw: u8 = match self.ycc_quantization {
            YccQuantization::LimitedRange => 0,
            YccQuantization::FullRange => 1,
        };
        let cn_raw: u8 = match self.it_content_type {
            ItContentType::Graphics => 0,
            ItContentType::Photo => 1,
            ItContentType::Cinema => 2,
            ItContentType::Game => 3,
        };
        hp[7] = (yq_raw << 6) | (cn_raw << 4) | (self.pixel_repetition & 0x0F);

        // PB6–PB13: bar data (little-endian u16 pairs).
        let [lo, hi] = self.top_bar.to_le_bytes();
        hp[8] = lo;
        hp[9] = hi;
        let [lo, hi] = self.bottom_bar.to_le_bytes();
        hp[10] = lo;
        hp[11] = hi;
        let [lo, hi] = self.left_bar.to_le_bytes();
        hp[12] = lo;
        hp[13] = hi;
        let [lo, hi] = self.right_bar.to_le_bytes();
        hp[14] = lo;
        hp[15] = hi;

        let checksum = crate::checksum::compute_checksum(&hp);

        let mut packet = [0u8; 31];
        packet[..3].copy_from_slice(&hp[..3]);
        packet[3] = checksum;
        packet[4..].copy_from_slice(&hp[3..]);

        SinglePacketIter::new(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::IntoPackets;

    fn full_frame() -> AviInfoFrame {
        AviInfoFrame {
            color_format: ColorFormat::YCbCr444,
            active_format_present: true,
            bar_info: BarInfo::BothPresent,
            scan_info: ScanInfo::Underscanned,
            colorimetry: Colorimetry::Extended,
            extended_colorimetry: ExtendedColorimetry::Bt2020YCC,
            picture_aspect_ratio: PictureAspectRatio::SixteenByNine,
            active_format_aspect_ratio: 0x08,
            it_content: true,
            rgb_quantization: RgbQuantization::FullRange,
            non_uniform_scaling: NonUniformScaling::None,
            vic: 16,
            ycc_quantization: YccQuantization::LimitedRange,
            it_content_type: ItContentType::Cinema,
            pixel_repetition: 0,
            top_bar: 0x1234,
            bottom_bar: 0x5678,
            left_bar: 0x9ABC,
            right_bar: 0xDEF0,
        }
    }

    #[test]
    fn round_trip() {
        let frame = full_frame();
        let packet = frame.clone().into_packets().next().unwrap();
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value, frame);
    }

    #[test]
    fn checksum_mismatch_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[3] = packet[3].wrapping_add(1);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(
            decoded
                .iter_warnings()
                .any(|w| matches!(w, AviWarning::ChecksumMismatch { .. }))
        );
        assert_eq!(decoded.value, full_frame());
    }

    #[test]
    fn truncated_length_is_error() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[2] = 28;
        assert!(matches!(
            AviInfoFrame::decode(&packet),
            Err(DecodeError::Truncated {
                claimed: 28,
                available: 27
            })
        ));
    }

    #[test]
    fn reserved_bit_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[7] |= 0x80; // set reserved bit 7 of PB4
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(
            decoded
                .iter_warnings()
                .any(|w| matches!(w, AviWarning::ReservedFieldNonZero { byte: 7, bit: 7 }))
        );
    }

    #[test]
    fn color_format_variants_round_trip() {
        for fmt in [
            ColorFormat::Rgb444,
            ColorFormat::YCbCr422,
            ColorFormat::YCbCr420,
        ] {
            let frame = AviInfoFrame {
                color_format: fmt,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.color_format, fmt);
        }
    }

    #[test]
    fn scan_info_and_colorimetry_variants_round_trip() {
        for (scan, c) in [
            (ScanInfo::NoData, Colorimetry::NoData),
            (ScanInfo::Overscanned, Colorimetry::Bt601),
            (ScanInfo::Underscanned, Colorimetry::Bt709),
        ] {
            let frame = AviInfoFrame {
                scan_info: scan,
                colorimetry: c,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.scan_info, scan);
            assert_eq!(decoded.value.colorimetry, c);
        }
    }

    #[test]
    fn bar_info_and_aspect_ratio_variants_round_trip() {
        for (bar, aspect) in [
            (BarInfo::NotPresent, PictureAspectRatio::NoData),
            (
                BarInfo::VerticalBarsPresent,
                PictureAspectRatio::FourByThree,
            ),
            (
                BarInfo::HorizontalBarsPresent,
                PictureAspectRatio::SixteenByNine,
            ),
        ] {
            let frame = AviInfoFrame {
                bar_info: bar,
                picture_aspect_ratio: aspect,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.bar_info, bar);
            assert_eq!(decoded.value.picture_aspect_ratio, aspect);
        }
    }

    #[test]
    fn quantization_and_scaling_variants_round_trip() {
        for (rgb_q, ycc_q, sc) in [
            (
                RgbQuantization::Default,
                YccQuantization::FullRange,
                NonUniformScaling::Horizontal,
            ),
            (
                RgbQuantization::LimitedRange,
                YccQuantization::LimitedRange,
                NonUniformScaling::Vertical,
            ),
            (
                RgbQuantization::FullRange,
                YccQuantization::LimitedRange,
                NonUniformScaling::Both,
            ),
        ] {
            let frame = AviInfoFrame {
                rgb_quantization: rgb_q,
                ycc_quantization: ycc_q,
                non_uniform_scaling: sc,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.rgb_quantization, rgb_q);
            assert_eq!(decoded.value.ycc_quantization, ycc_q);
            assert_eq!(decoded.value.non_uniform_scaling, sc);
        }
    }

    #[test]
    fn it_content_type_variants_round_trip() {
        for cn in [
            ItContentType::Graphics,
            ItContentType::Photo,
            ItContentType::Game,
        ] {
            let frame = AviInfoFrame {
                it_content: true,
                it_content_type: cn,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.it_content_type, cn);
        }
    }

    #[test]
    fn extended_colorimetry_variants_round_trip() {
        for ec in [
            ExtendedColorimetry::XvYCC601,
            ExtendedColorimetry::XvYCC709,
            ExtendedColorimetry::SyCC601,
            ExtendedColorimetry::OpYCC601,
            ExtendedColorimetry::OpRgb,
            ExtendedColorimetry::Bt2020cYCC,
            ExtendedColorimetry::Bt2020YCC,
            ExtendedColorimetry::AdditionalColorimetryExtension,
        ] {
            let frame = AviInfoFrame {
                colorimetry: Colorimetry::Extended,
                extended_colorimetry: ec,
                ..full_frame()
            };
            let packet = frame.clone().into_packets().next().unwrap();
            let decoded = AviInfoFrame::decode(&packet).unwrap();
            assert_eq!(decoded.value.extended_colorimetry, ec);
        }
    }

    #[test]
    fn unknown_scan_info_warns() {
        // S = 0b11 is reserved; decode should warn and fall back to NoData.
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[4] = (packet[4] & !0x03) | 0x03; // set S[1:0] = 0b11
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            AviWarning::UnknownEnumValue {
                field: "scan_info",
                raw: 3
            }
        )));
        assert_eq!(decoded.value.scan_info, ScanInfo::NoData);
    }

    #[test]
    fn unknown_color_format_warns() {
        // Y[2:0] = 0b101 (5) is not a defined ColorFormat.
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[4] = (packet[4] & !0xE0) | (5u8 << 5);
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            AviWarning::UnknownEnumValue {
                field: "color_format",
                raw: 5
            }
        )));
        assert_eq!(decoded.value.color_format, ColorFormat::Rgb444);
    }

    #[test]
    fn unknown_rgb_quantization_warns() {
        // Q[1:0] = 0b11 is reserved.
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[6] = (packet[6] & !0x0C) | 0x0C; // set Q[1:0] = 0b11
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            AviWarning::UnknownEnumValue {
                field: "rgb_quantization",
                raw: 3
            }
        )));
        assert_eq!(decoded.value.rgb_quantization, RgbQuantization::Default);
    }

    #[test]
    fn unknown_ycc_quantization_warns() {
        // YQ[1:0] = 0b10 is reserved.
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[8] = (packet[8] & !0xC0) | 0x80; // set YQ[1:0] = 0b10
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            AviWarning::UnknownEnumValue {
                field: "ycc_quantization",
                raw: 2
            }
        )));
        assert_eq!(
            decoded.value.ycc_quantization,
            YccQuantization::LimitedRange
        );
    }

    #[test]
    fn short_packet_leaves_bar_data_zeroed() {
        // Encode a minimal packet with length=5 (no bar data).
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[2] = 5;
        // Recompute checksum.
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert_eq!(decoded.value.top_bar, 0);
        assert_eq!(decoded.value.bottom_bar, 0);
        assert_eq!(decoded.value.left_bar, 0);
        assert_eq!(decoded.value.right_bar, 0);
    }
}
