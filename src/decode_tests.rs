use super::*;
use crate::audio::{
    AudioCodingType, AudioInfoFrame, ChannelCount, LfePlaybackLevel, SampleFrequency, SampleSize,
};
use crate::avi::{
    AviInfoFrame, BarInfo, Colorimetry, ExtendedColorimetry, ItContentType, NonUniformScaling,
    PictureAspectRatio, RgbQuantization, ScanInfo, YccQuantization,
};
use crate::decode;
use crate::encode::IntoPackets;
use crate::error::DecodeError;
use crate::hdmi_forum_vsi::HdmiForumVsi;
use crate::hdr_static::{Eotf, HdrStaticInfoFrame, StaticMetadata, StaticMetadataType1};
use crate::warn::Warning;
use display_types::cea861::hdmi_forum::HdmiDscMaxSlices;
use display_types::{ColorFormat, HdmiForumFrl};

fn avi_packet() -> [u8; 31] {
    AviInfoFrame {
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
        vic: 16,
        ycc_quantization: YccQuantization::LimitedRange,
        it_content_type: ItContentType::Graphics,
        pixel_repetition: 0,
        top_bar: 0,
        bottom_bar: 0,
        left_bar: 0,
        right_bar: 0,
    }
    .into_packets()
    .value
    .next()
    .unwrap()
}

fn audio_packet() -> [u8; 31] {
    AudioInfoFrame {
        coding_type: AudioCodingType::Lpcm,
        channel_count: ChannelCount::Count(2),
        sample_freq: SampleFrequency::Hz48000,
        sample_size: SampleSize::Bits16,
        coding_ext: 0,
        channel_allocation: 0,
        lfe_playback_level: LfePlaybackLevel::NoInfo,
        downmix_inhibit: false,
    }
    .into_packets()
    .value
    .next()
    .unwrap()
}

fn hdr_static_packet() -> [u8; 31] {
    HdrStaticInfoFrame {
        eotf: Eotf::Pq,
        metadata: StaticMetadata::Type1(StaticMetadataType1 {
            primaries_green: [0, 0],
            primaries_blue: [0, 0],
            primaries_red: [0, 0],
            white_point: [0, 0],
            max_mastering_luminance: 1000,
            min_mastering_luminance: 1,
            max_cll: 1000,
            max_fall: 400,
        }),
    }
    .into_packets()
    .value
    .next()
    .unwrap()
}

fn hdmi_forum_vsi_packet() -> [u8; 31] {
    HdmiForumVsi {
        allm: true,
        frl_rate: HdmiForumFrl::NotSupported,
        fapa_start_location: false,
        fva: false,
        vrr_en: false,
        m_const: false,
        qms_en: false,
        neg_mvrr: false,
        m_vrr: 0,
        dsc_1p2: false,
        dsc_native_420: false,
        dsc_all_bpc: false,
        dsc_max_frl_rate: HdmiForumFrl::NotSupported,
        dsc_max_slices: HdmiDscMaxSlices::NotSupported,
        dsc_10bpc: false,
        dsc_12bpc: false,
    }
    .into_packets()
    .value
    .next()
    .unwrap()
}

#[test]
fn decode_avi_packet() {
    let packet = avi_packet();
    let result = decode(&packet).unwrap();
    assert!(matches!(result.value, InfoFramePacket::Avi(_)));
    assert!(result.iter_warnings().next().is_none());
}

#[test]
fn decode_audio_packet() {
    let packet = audio_packet();
    let result = decode(&packet).unwrap();
    assert!(matches!(result.value, InfoFramePacket::Audio(_)));
    assert!(result.iter_warnings().next().is_none());
}

#[test]
fn decode_hdr_static_packet() {
    let packet = hdr_static_packet();
    let result = decode(&packet).unwrap();
    assert!(matches!(result.value, InfoFramePacket::HdrStatic(_)));
    assert!(result.iter_warnings().next().is_none());
}

#[test]
fn decode_hdmi_forum_vsi_packet() {
    let packet = hdmi_forum_vsi_packet();
    let result = decode(&packet).unwrap();
    assert!(matches!(result.value, InfoFramePacket::HdmiForumVsi(_)));
    assert!(result.iter_warnings().next().is_none());
}

#[test]
fn decode_vsif_unknown_oui_is_unknown() {
    let mut packet = [0u8; 31];
    packet[0] = 0x81; // VSIF type code
    packet[1] = 0x01;
    packet[2] = 8;
    // OUI bytes (packet[4..7]) left as 0x00 — not the HDMI Forum OUI
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let result = decode(&packet).unwrap();
    assert!(matches!(
        result.value,
        InfoFramePacket::Unknown {
            type_code: 0x81,
            ..
        }
    ));
}

#[test]
fn decode_dynamic_hdr_fragment() {
    let mut packet = [0u8; 31];
    packet[0] = 0x20; // Dynamic HDR type code
    packet[1] = 0x01;
    packet[2] = 7; // length = 4 overhead + 3 chunk bytes
    packet[4] = 2; // seq_num
    packet[5] = 0x1E; // total_bytes low
    packet[6] = 0x00; // total_bytes high
    packet[7] = 0x04; // format_id
    packet[8] = 0xAA;
    packet[9] = 0xBB;
    packet[10] = 0xCC;
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let result = decode(&packet).unwrap();
    assert!(result.iter_warnings().next().is_none());
    if let InfoFramePacket::DynamicHdrFragment(frag) = result.value {
        assert_eq!(frag.seq_num, 2);
        assert_eq!(frag.total_bytes, 0x001E);
        assert_eq!(frag.format_id, 0x04);
        assert_eq!(frag.chunk_len, 3);
        assert_eq!(&frag.chunk[..3], &[0xAA, 0xBB, 0xCC]);
    } else {
        panic!("expected DynamicHdrFragment variant");
    }
}

#[test]
fn decode_unknown_type_code() {
    let mut packet = [0u8; 31];
    packet[0] = 0x55;
    packet[1] = 0x01;
    packet[2] = 4;
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let result = decode(&packet).unwrap();
    assert!(matches!(
        result.value,
        InfoFramePacket::Unknown {
            type_code: 0x55,
            ..
        }
    ));
}

#[test]
fn decode_truncated_returns_error() {
    let mut packet = [0u8; 31];
    packet[0] = 0x82;
    packet[2] = 28; // > 27
    assert!(matches!(
        decode(&packet),
        Err(DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn decode_unknown_payload_round_trips() {
    let mut packet = [0u8; 31];
    packet[0] = 0xAB;
    packet[1] = 0x03;
    packet[2] = 4;
    for i in 0..27usize {
        packet[4 + i] = i as u8;
    }
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let result = decode(&packet).unwrap();
    if let InfoFramePacket::Unknown {
        type_code,
        version,
        payload,
    } = result.value
    {
        assert_eq!(type_code, 0xAB);
        assert_eq!(version, 0x03);
        assert_eq!(&payload[..], &packet[4..31]);
    } else {
        panic!("expected Unknown variant");
    }
}

#[test]
fn decode_warning_lifted_to_unified_type() {
    let mut packet = avi_packet();
    packet[3] = packet[3].wrapping_add(1); // corrupt checksum
    let result = decode(&packet).unwrap();
    assert!(result.iter_warnings().any(|w| matches!(w, Warning::Avi(_))));
}

#[test]
fn decode_audio_warning_lifted() {
    let mut packet = audio_packet();
    packet[3] = packet[3].wrapping_add(1);
    let result = decode(&packet).unwrap();
    assert!(
        result
            .iter_warnings()
            .any(|w| matches!(w, Warning::Audio(_)))
    );
}

#[test]
fn decode_hdr_static_warning_lifted() {
    let mut packet = hdr_static_packet();
    packet[3] = packet[3].wrapping_add(1);
    let result = decode(&packet).unwrap();
    assert!(
        result
            .iter_warnings()
            .any(|w| matches!(w, Warning::HdrStatic(_)))
    );
}

#[test]
fn decode_hdmi_forum_vsi_warning_lifted() {
    let mut packet = hdmi_forum_vsi_packet();
    packet[3] = packet[3].wrapping_add(1);
    let result = decode(&packet).unwrap();
    assert!(
        result
            .iter_warnings()
            .any(|w| matches!(w, Warning::HdmiForumVsi(_)))
    );
}

#[test]
fn decode_dynamic_hdr_warning_lifted() {
    let mut packet = [0u8; 31];
    packet[0] = 0x20; // Dynamic HDR type code
    packet[1] = 0x01;
    packet[2] = 4; // minimum valid length
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    packet[3] = packet[3].wrapping_add(1); // corrupt checksum
    let result = decode(&packet).unwrap();
    assert!(
        result
            .iter_warnings()
            .any(|w| matches!(w, Warning::DynamicHdr(_)))
    );
}
