//! Round-trip example: construct one of each InfoFrame type, encode to wire
//! packets, decode from those packets, and assert field equality.
//!
//! Run with:
//!
//! ```text
//! cargo run --example roundtrip
//! ```

use cartouche::audio::{
    AudioCodingType, AudioInfoFrame, ChannelCount, LfePlaybackLevel, SampleFrequency, SampleSize,
};
use cartouche::avi::{
    AviInfoFrame, BarInfo, Colorimetry, ExtendedColorimetry, ItContentType, NonUniformScaling,
    PictureAspectRatio, RgbQuantization, ScanInfo, YccQuantization,
};
use cartouche::dynamic_hdr::{DynamicHdrFragment, DynamicHdrInfoFrame};
use cartouche::encode::IntoPackets;
use cartouche::hdmi_forum_vsi::HdmiForumVsi;
use cartouche::hdr_static::{Eotf, HdrStaticInfoFrame, StaticMetadata, StaticMetadataType1};
use display_types::cea861::hdmi_forum::HdmiDscMaxSlices;
use display_types::{ColorFormat, HdmiForumFrl};

fn main() {
    avi_round_trip();
    audio_round_trip();
    hdr_static_round_trip();
    hdmi_forum_vsi_round_trip();
    dynamic_hdr_fragment_decode();

    println!("All round-trips passed.");
}

fn avi_round_trip() {
    let original = AviInfoFrame {
        color_format: ColorFormat::YCbCr444,
        active_format_present: true,
        bar_info: BarInfo::BothPresent,
        scan_info: ScanInfo::Underscanned,
        colorimetry: Colorimetry::Extended,
        extended_colorimetry: ExtendedColorimetry::Bt2020YCC,
        picture_aspect_ratio: PictureAspectRatio::SixteenByNine,
        active_format_aspect_ratio: 0x08,
        it_content: false,
        rgb_quantization: RgbQuantization::Default,
        non_uniform_scaling: NonUniformScaling::None,
        vic: 97,
        ycc_quantization: YccQuantization::LimitedRange,
        it_content_type: ItContentType::Graphics,
        pixel_repetition: 0,
        top_bar: 0,
        bottom_bar: 0,
        left_bar: 0,
        right_bar: 0,
    };

    let packet = original.clone().into_packets().value.next().unwrap();
    let decoded = AviInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded.iter_warnings().next().is_none(),
        "unexpected warnings"
    );
    assert_eq!(decoded.value, original);
    println!("  AVI:            ok  (type code 0x{:02X})", packet[0]);
}

fn audio_round_trip() {
    let original = AudioInfoFrame {
        coding_type: AudioCodingType::Lpcm,
        channel_count: ChannelCount::Count(8),
        sample_freq: SampleFrequency::Hz192000,
        sample_size: SampleSize::Bits24,
        coding_ext: 0,
        channel_allocation: 0x13,
        lfe_playback_level: LfePlaybackLevel::Plus10Db,
        downmix_inhibit: true,
    };

    let packet = original.clone().into_packets().value.next().unwrap();
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded.iter_warnings().next().is_none(),
        "unexpected warnings"
    );
    assert_eq!(decoded.value, original);
    println!("  Audio:          ok  (type code 0x{:02X})", packet[0]);
}

fn hdr_static_round_trip() {
    let original = HdrStaticInfoFrame {
        eotf: Eotf::Pq,
        metadata: StaticMetadata::Type1(StaticMetadataType1 {
            primaries_green: [15000, 30000],
            primaries_blue: [7500, 3000],
            primaries_red: [34000, 16000],
            white_point: [15635, 16450],
            max_mastering_luminance: 1000,
            min_mastering_luminance: 1,
            max_cll: 1000,
            max_fall: 400,
        }),
    };

    let packet = original.clone().into_packets().value.next().unwrap();
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded.iter_warnings().next().is_none(),
        "unexpected warnings"
    );
    assert_eq!(decoded.value, original);
    println!("  HDR Static:     ok  (type code 0x{:02X})", packet[0]);
}

fn hdmi_forum_vsi_round_trip() {
    let original = HdmiForumVsi {
        allm: true,
        frl_rate: HdmiForumFrl::Rate12Gbps4Lanes,
        fapa_start_location: false,
        fva: false,
        vrr_en: true,
        m_const: false,
        qms_en: false,
        neg_mvrr: false,
        m_vrr: 120,
        dsc_1p2: true,
        dsc_native_420: false,
        dsc_all_bpc: false,
        dsc_max_frl_rate: HdmiForumFrl::Rate6Gbps4Lanes,
        dsc_max_slices: HdmiDscMaxSlices::Slices8At340Mhz,
        dsc_10bpc: true,
        dsc_12bpc: false,
    };

    let packet = original.clone().into_packets().value.next().unwrap();
    let decoded = HdmiForumVsi::decode(&packet).unwrap();
    assert!(
        decoded.iter_warnings().next().is_none(),
        "unexpected warnings"
    );
    assert_eq!(decoded.value, original);
    println!("  HDMI Forum VSI: ok  (type code 0x{:02X})", packet[0]);
}

fn dynamic_hdr_fragment_decode() {
    // Build a single-packet Dynamic HDR sequence and decode both the fragment
    // and the assembled frame.  Full round-trip encoding is deferred until a
    // concrete format variant (HDR10+, SL-HDR) is implemented.
    let mut packet = [0u8; 31];
    packet[0] = 0x20; // Dynamic HDR type code
    packet[1] = 0x01; // version
    packet[2] = 4 + 10; // 4 overhead + 10 chunk bytes
    packet[4] = 0; // seq_num
    packet[5] = 10; // total_bytes low
    packet[6] = 0; // total_bytes high
    packet[7] = 0x04; // format_id (HDR10+)
    for (i, b) in packet[8..18].iter_mut().enumerate() {
        *b = i as u8;
    }
    let sum: u8 = packet.iter().fold(0u8, |a, &x| a.wrapping_add(x));
    packet[3] = 0u8.wrapping_sub(sum);

    let frag = DynamicHdrFragment::decode(&packet).unwrap();
    assert_eq!(frag.value.seq_num, 0);
    assert_eq!(frag.value.total_bytes, 10);
    assert_eq!(frag.value.format_id, 0x04);
    assert_eq!(frag.value.chunk_len, 10);

    let frame = DynamicHdrInfoFrame::decode_sequence(&[packet]).unwrap();
    assert_eq!(
        frame.value,
        DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            payload: (0u8..10).collect(),
        }
    );

    println!(
        "  Dynamic HDR:    ok  (type code 0x{:02X}, fragment decode only)",
        packet[0]
    );
}
