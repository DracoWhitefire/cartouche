use super::*;

use crate::encode::IntoPackets;

fn default_frame() -> AudioInfoFrame {
    AudioInfoFrame {
        coding_type: AudioCodingType::Lpcm,
        channel_count: ChannelCount::Count(8),
        sample_freq: SampleFrequency::Hz48000,
        sample_size: SampleSize::Bits24,
        coding_ext: 0,
        channel_allocation: 0x13, // FL FR LFE FC RL RR RLC RRC
        lfe_playback_level: LfePlaybackLevel::Ref0Db,
        downmix_inhibit: true,
    }
}

#[test]
fn round_trip() {
    let frame = default_frame();
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(decoded.value, frame);
}

#[test]
fn checksum_mismatch_warning() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[3] = packet[3].wrapping_add(1); // corrupt the checksum byte
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    let warnings: alloc::vec::Vec<_> = decoded.iter_warnings().collect();
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, AudioWarning::ChecksumMismatch { .. }))
    );
    // Frame is still returned intact
    assert_eq!(decoded.value, default_frame());
}

#[test]
fn truncated_length_is_error() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[2] = 28; // length > 27
    assert!(matches!(
        AudioInfoFrame::decode(&packet),
        Err(crate::error::DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn reserved_bit_warning() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[4] |= 0x80; // set reserved bit 7 of PB1
    // Recompute checksum to isolate the reserved-bit warning.
    let sum: u8 = packet[..31].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, AudioWarning::ReservedFieldNonZero { byte: 4, bit: 7 }))
    );
}

#[test]
fn unknown_lfe_playback_level_warns_and_falls_back() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    // LSV occupies PB5 bits [6:3]; set value 0x0F (15) which is out of spec.
    packet[8] = (packet[8] & 0x87) | (0x0F << 3);
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        AudioWarning::UnknownEnumValue {
            field: "lfe_playback_level",
            raw: 15
        }
    )));
    assert_eq!(decoded.value.lfe_playback_level, LfePlaybackLevel::NoInfo);
}

#[test]
fn coding_types_round_trip() {
    for ct in [
        AudioCodingType::Ac3,
        AudioCodingType::Mpeg1,
        AudioCodingType::Mp3,
        AudioCodingType::Mpeg2Multichannel,
        AudioCodingType::AacLc,
        AudioCodingType::Dts,
        AudioCodingType::Atrac,
        AudioCodingType::OneBitAudio,
        AudioCodingType::EnhancedAc3,
        AudioCodingType::DtsHd,
        AudioCodingType::MlpTrueHd,
        AudioCodingType::Dst,
        AudioCodingType::WmaPro,
        AudioCodingType::Extension,
    ] {
        let frame = AudioInfoFrame {
            coding_type: ct,
            ..default_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
        let decoded = AudioInfoFrame::decode(&packet).unwrap();
        assert_eq!(decoded.value.coding_type, ct);
    }
}

#[test]
fn sample_freq_and_size_variants_round_trip() {
    for (sf, ss) in [
        (SampleFrequency::ReferToStream, SampleSize::ReferToStream),
        (SampleFrequency::Hz32000, SampleSize::Bits16),
        (SampleFrequency::Hz44100, SampleSize::Bits20),
        (SampleFrequency::Hz48000, SampleSize::Bits24),
        (SampleFrequency::Hz88200, SampleSize::Bits16),
        (SampleFrequency::Hz96000, SampleSize::Bits16),
        (SampleFrequency::Hz176400, SampleSize::Bits16),
        (SampleFrequency::Hz192000, SampleSize::Bits16),
    ] {
        let frame = AudioInfoFrame {
            sample_freq: sf,
            sample_size: ss,
            ..default_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
        let decoded = AudioInfoFrame::decode(&packet).unwrap();
        assert_eq!(decoded.value.sample_freq, sf);
        assert_eq!(decoded.value.sample_size, ss);
    }
}

#[test]
fn lfe_playback_levels_round_trip() {
    for lsv in [
        LfePlaybackLevel::NoInfo,
        LfePlaybackLevel::Plus10Db,
        LfePlaybackLevel::Ref0Db,
    ] {
        let frame = AudioInfoFrame {
            lfe_playback_level: lsv,
            ..default_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
        let decoded = AudioInfoFrame::decode(&packet).unwrap();
        assert_eq!(decoded.value.lfe_playback_level, lsv);
    }
}

#[test]
fn reserved_pb2_bits_warning() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[5] |= 0x80; // set reserved bit 7 of PB2
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, AudioWarning::ReservedFieldNonZero { byte: 5, bit: 7 }))
    );
}

#[test]
fn reserved_pb3_bits_warning() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[6] |= 0x20; // set reserved bit 5 of PB3
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, AudioWarning::ReservedFieldNonZero { byte: 6, bit: 5 }))
    );
}

#[test]
fn reserved_pb5_bits_warning() {
    let mut packet = default_frame().into_packets().value.next().unwrap();
    packet[8] |= 0x01; // set reserved bit 0 of PB5
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, AudioWarning::ReservedFieldNonZero { byte: 8, bit: 0 }))
    );
}

#[test]
fn refer_to_stream_fields_round_trip() {
    let frame = AudioInfoFrame {
        coding_type: AudioCodingType::ReferToStream,
        channel_count: ChannelCount::ReferToStream,
        sample_freq: SampleFrequency::ReferToStream,
        sample_size: SampleSize::ReferToStream,
        coding_ext: 0,
        channel_allocation: 0,
        lfe_playback_level: LfePlaybackLevel::NoInfo,
        downmix_inhibit: false,
    };
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = AudioInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(decoded.value, frame);
}

#[cfg(feature = "serde")]
#[test]
fn audio_info_frame_round_trip() {
    use super::*;
    let frame = AudioInfoFrame {
        coding_type: AudioCodingType::Lpcm,
        channel_count: ChannelCount::Count(6),
        sample_freq: SampleFrequency::Hz48000,
        sample_size: SampleSize::Bits24,
        coding_ext: 0,
        channel_allocation: 0x0B,
        lfe_playback_level: LfePlaybackLevel::Plus10Db,
        downmix_inhibit: true,
    };
    let json = serde_json::to_string(&frame).unwrap();
    let back: AudioInfoFrame = serde_json::from_str(&json).unwrap();
    assert_eq!(frame, back);
}
