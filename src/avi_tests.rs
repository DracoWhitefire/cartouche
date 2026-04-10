use display_types::ColorFormat;

use super::*;

use crate::encode::IntoPackets;
use crate::error::DecodeError;
use crate::warn::AviWarning;

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
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = AviInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(decoded.value, frame);
}

#[test]
fn checksum_mismatch_warning() {
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
    packet[2] = 28;
    assert!(matches!(
        AviInfoFrame::decode(&packet),
        Err(DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn reserved_bit_warning() {
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
        let packet = frame.clone().into_packets().value.next().unwrap();
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
        (ScanInfo::NoData, Colorimetry::Extended),
    ] {
        let frame = AviInfoFrame {
            scan_info: scan,
            colorimetry: c,
            ..full_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
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
        (BarInfo::BothPresent, PictureAspectRatio::SixteenByNine),
    ] {
        let frame = AviInfoFrame {
            bar_info: bar,
            picture_aspect_ratio: aspect,
            ..full_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
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
        let packet = frame.clone().into_packets().value.next().unwrap();
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
        ItContentType::Cinema,
        ItContentType::Game,
    ] {
        let frame = AviInfoFrame {
            it_content: true,
            it_content_type: cn,
            ..full_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
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
        let packet = frame.clone().into_packets().value.next().unwrap();
        let decoded = AviInfoFrame::decode(&packet).unwrap();
        assert_eq!(decoded.value.extended_colorimetry, ec);
    }
}

#[test]
fn unknown_scan_info_warns() {
    // S = 0b11 is reserved; decode should warn and fall back to NoData.
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
fn unknown_picture_aspect_ratio_warns() {
    // M[1:0] = 0b11 is reserved; decode should warn and fall back to NoData.
    let mut packet = full_frame().into_packets().value.next().unwrap();
    packet[5] = (packet[5] & !0x30) | 0x30; // set M[1:0] = 0b11
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AviInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        AviWarning::UnknownEnumValue {
            field: "picture_aspect_ratio",
            raw: 3
        }
    )));
    assert_eq!(
        decoded.value.picture_aspect_ratio,
        PictureAspectRatio::NoData
    );
}

#[test]
fn vic_out_of_range_warns_on_encode() {
    // VIC is a 7-bit field; values > 127 are truncated on the wire and
    // should produce an UnknownEnumValue warning.
    let frame = AviInfoFrame {
        vic: 200,
        ..full_frame()
    };
    let mut encoded = frame.into_packets();
    assert!(encoded.iter_warnings().any(|w| matches!(
        w,
        AviWarning::UnknownEnumValue {
            field: "vic",
            raw: 200
        }
    )));
    // Wire value is 200 & 0x7F = 72.
    let packet = encoded.value.next().unwrap();
    assert_eq!(packet[7] & 0x7F, 72);
}

#[test]
fn short_packet_leaves_bar_data_zeroed() {
    // Encode a minimal packet with length=5 (no bar data).
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
