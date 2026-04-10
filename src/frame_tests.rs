use super::*;
use crate::audio::{
    AudioCodingType, AudioInfoFrame, ChannelCount, LfePlaybackLevel, SampleFrequency, SampleSize,
};
use crate::avi::{
    AviInfoFrame, BarInfo, Colorimetry, ExtendedColorimetry, ItContentType, NonUniformScaling,
    PictureAspectRatio, RgbQuantization, ScanInfo, YccQuantization,
};
use crate::dynamic_hdr::DynamicHdrInfoFrame;
use crate::hdmi_forum_vsi::HdmiForumVsi;
use crate::hdr_static::{Eotf, HdrStaticInfoFrame, StaticMetadata, StaticMetadataType1};
#[cfg(any(feature = "alloc", feature = "std"))]
use alloc::vec;
use display_types::cea861::hdmi_forum::HdmiDscMaxSlices;
use display_types::{ColorFormat, HdmiForumFrl};

#[test]
fn avi_variant_produces_one_packet() {
    let frame = InfoFrame::Avi(AviInfoFrame {
        color_format: ColorFormat::YCbCr444,
        active_format_present: false,
        bar_info: BarInfo::NotPresent,
        scan_info: ScanInfo::Underscanned,
        colorimetry: Colorimetry::Bt709,
        extended_colorimetry: ExtendedColorimetry::XvYCC601,
        picture_aspect_ratio: PictureAspectRatio::SixteenByNine,
        active_format_aspect_ratio: 0x08,
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
    });
    let mut iter = frame.into_packets().value;
    let packet = iter.next().expect("expected one packet");
    assert_eq!(packet[0], 0x82); // AVI type code
    assert!(iter.next().is_none());
}

#[test]
fn audio_variant_produces_one_packet() {
    let frame = InfoFrame::Audio(AudioInfoFrame {
        coding_type: AudioCodingType::Lpcm,
        channel_count: ChannelCount::Count(2),
        sample_freq: SampleFrequency::Hz48000,
        sample_size: SampleSize::Bits16,
        coding_ext: 0,
        channel_allocation: 0,
        lfe_playback_level: LfePlaybackLevel::NoInfo,
        downmix_inhibit: false,
    });
    let mut iter = frame.into_packets().value;
    let packet = iter.next().expect("expected one packet");
    assert_eq!(packet[0], 0x84); // Audio type code
    assert!(iter.next().is_none());
}

#[test]
fn hdr_static_variant_produces_one_packet() {
    let frame = InfoFrame::HdrStatic(HdrStaticInfoFrame {
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
    });
    let mut iter = frame.into_packets().value;
    let packet = iter.next().expect("expected one packet");
    assert_eq!(packet[0], 0x87); // HDR Static type code
    assert!(iter.next().is_none());
}

#[test]
fn hdmi_forum_vsi_variant_produces_one_packet() {
    let frame = InfoFrame::HdmiForumVsi(HdmiForumVsi {
        allm: false,
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
    });
    let mut iter = frame.into_packets().value;
    let packet = iter.next().expect("expected one packet");
    assert_eq!(packet[0], 0x81); // VSIF type code
    assert!(iter.next().is_none());
}

#[test]
fn dynamic_hdr_empty_payload_yields_no_packets() {
    let frame = InfoFrame::DynamicHdr(DynamicHdrInfoFrame::Unknown {
        format_id: 0x04,
        #[cfg(any(feature = "alloc", feature = "std"))]
        payload: vec![],
    });
    assert!(frame.into_packets().value.next().is_none());
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn info_frame_dynamic_hdr_round_trip() {
    let original_payload: alloc::vec::Vec<u8> = (0u8..30).collect();
    let frame = InfoFrame::DynamicHdr(DynamicHdrInfoFrame::Unknown {
        format_id: 0x0A,
        payload: original_payload.clone(),
    });
    let packets: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    // 30 bytes → 2 packets (23 + 7)
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0][0], 0x20); // Dynamic HDR type code
    let decoded = DynamicHdrInfoFrame::decode_sequence(&packets).unwrap();
    assert_eq!(
        decoded.value,
        DynamicHdrInfoFrame::Unknown {
            format_id: 0x0A,
            payload: original_payload,
        }
    );
}

#[test]
fn unknown_variant_produces_one_packet() {
    let mut payload = [0u8; 27];
    payload[0] = 0xDE;
    payload[1] = 0xAD;
    let frame = InfoFrame::Unknown {
        type_code: 0xFE,
        version: 0x02,
        payload,
    };
    let mut iter = frame.into_packets().value;
    let packet = iter.next().expect("expected one packet");
    assert_eq!(packet[0], 0xFE); // type code
    assert_eq!(packet[1], 0x02); // version
    assert_eq!(packet[2], 27); // length
    assert_eq!(packet[4], 0xDE); // first payload byte
    assert_eq!(packet[5], 0xAD); // second payload byte
    assert!(iter.next().is_none());
}

#[test]
fn unknown_variant_checksum_is_valid() {
    let frame = InfoFrame::Unknown {
        type_code: 0xCC,
        version: 0x01,
        payload: [0u8; 27],
    };
    let packet = frame.into_packets().value.next().unwrap();
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    assert_eq!(sum, 0, "checksum must make all-bytes sum equal 0");
}
