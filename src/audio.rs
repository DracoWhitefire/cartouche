/// Audio coding type (CT field, PB1 bits 6–3).
///
/// Identifies the audio format being transmitted. `ReferToStream` defers to
/// the audio bitstream's own header; this is the common case for compressed
/// formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AudioCodingType {
    /// Coding type is indicated in the audio stream header (CT = 0).
    ReferToStream,
    /// Linear PCM (CT = 1).
    Lpcm,
    /// Dolby AC-3 (CT = 2).
    Ac3,
    /// MPEG-1 layers 1 & 2 (CT = 3).
    Mpeg1,
    /// MPEG-1 layer 3 / MP3 (CT = 4).
    Mp3,
    /// MPEG-2 multi-channel (CT = 5).
    Mpeg2Multichannel,
    /// AAC LC (CT = 6).
    AacLc,
    /// DTS (CT = 7).
    Dts,
    /// ATRAC (CT = 8).
    Atrac,
    /// One Bit Audio / DSD (CT = 9).
    OneBitAudio,
    /// Enhanced AC-3 / Dolby Digital Plus (CT = 10).
    EnhancedAc3,
    /// DTS-HD (CT = 11).
    DtsHd,
    /// MLP / Dolby TrueHD (CT = 12).
    MlpTrueHd,
    /// DST (CT = 13).
    Dst,
    /// WMA Pro (CT = 14).
    WmaPro,
    /// Coding type is in the CXT extension field (CT = 15).
    Extension,
}

/// Audio channel count (CC field, PB1 bits 2–0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChannelCount {
    /// Channel count is indicated in the audio stream header (CC = 0).
    ReferToStream,
    /// Explicit channel count (CC = 1–7).
    ///
    /// The stored value is the actual number of channels (2–8). On the wire
    /// the CC field carries `count - 1`. Values outside 2–8 are clamped on
    /// encode: `Count(0)` or `Count(1)` encode as CC = 0, which is
    /// indistinguishable from [`ReferToStream`](Self::ReferToStream);
    /// `Count(n)` for n > 8 encodes as CC = 7 (eight channels).
    Count(u8),
}

/// Audio sample frequency (SF field, PB2 bits 4–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleFrequency {
    /// Sample frequency is indicated in the audio stream header (SF = 0).
    ReferToStream,
    /// 32 kHz (SF = 1).
    Hz32000,
    /// 44.1 kHz (SF = 2).
    Hz44100,
    /// 48 kHz (SF = 3).
    Hz48000,
    /// 88.2 kHz (SF = 4).
    Hz88200,
    /// 96 kHz (SF = 5).
    Hz96000,
    /// 176.4 kHz (SF = 6).
    Hz176400,
    /// 192 kHz (SF = 7).
    Hz192000,
}

/// Audio sample size (SS field, PB2 bits 1–0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleSize {
    /// Sample size is indicated in the audio stream header (SS = 0).
    ReferToStream,
    /// 16-bit samples (SS = 1).
    Bits16,
    /// 20-bit samples (SS = 2).
    Bits20,
    /// 24-bit samples (SS = 3).
    Bits24,
}

/// LFE channel playback level relative to the other channels (LSV field,
/// PB5 bits 6–3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LfePlaybackLevel {
    /// No LFE playback level information (LSV = 0).
    NoInfo,
    /// LFE is played back at +10 dB relative to the other channels (LSV = 1).
    Plus10Db,
    /// LFE is played back at 0 dB relative to the other channels (LSV = 2).
    Ref0Db,
}

/// An Audio InfoFrame (CEA-861, type code 0x84).
///
/// Carries the audio format metadata required by the sink to configure its
/// audio decoder and speaker mapping. Transmitted alongside the audio stream.
///
/// The `coding_ext` field is only meaningful when `coding_type` is
/// [`AudioCodingType::Extension`]; in all other cases it should be `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioInfoFrame {
    /// Audio coding type (CT, PB1 bits 6–3).
    pub coding_type: AudioCodingType,
    /// Number of audio channels (CC, PB1 bits 2–0).
    pub channel_count: ChannelCount,
    /// Audio sample frequency (SF, PB2 bits 4–2).
    pub sample_freq: SampleFrequency,
    /// Audio sample size (SS, PB2 bits 1–0).
    pub sample_size: SampleSize,
    /// Audio coding extension type (CXT, PB3 bits 4–0).
    ///
    /// Only meaningful when `coding_type` is [`AudioCodingType::Extension`].
    pub coding_ext: u8,
    /// Channel/speaker allocation code (CA, PB4).
    ///
    /// Selects the speaker layout. See CEA-861 Table 20 for defined values.
    /// `0x00` is stereo (FL/FR); `0x01` adds LFE; `0x02` adds FC; etc.
    pub channel_allocation: u8,
    /// LFE playback level relative to the other channels (LSV, PB5 bits 6–3).
    pub lfe_playback_level: LfePlaybackLevel,
    /// Prohibit downmixing by the sink (DM_INH, PB5 bit 7).
    ///
    /// When `true`, the sink must not produce a downmixed stereo output from
    /// the multi-channel audio stream.
    pub downmix_inhibit: bool,
}

use crate::decoded::Decoded;
use crate::encode::{IntoPackets, SinglePacketIter};
use crate::error::DecodeError;
use crate::warn::AudioWarning;

impl AudioInfoFrame {
    /// Decode an Audio InfoFrame from a 31-byte wire packet.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry any of:
    /// - [`AudioWarning::ChecksumMismatch`] — the packet checksum did not verify.
    /// - [`AudioWarning::ReservedFieldNonZero`] — a reserved bit was set.
    /// - [`AudioWarning::UnknownEnumValue`] — a field contained a value not
    ///   defined in the spec.
    pub fn decode(packet: &[u8; 31]) -> Result<Decoded<AudioInfoFrame, AudioWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated { claimed: length });
        }

        let mut decoded = Decoded::new(AudioInfoFrame {
            coding_type: AudioCodingType::ReferToStream,
            channel_count: ChannelCount::ReferToStream,
            sample_freq: SampleFrequency::ReferToStream,
            sample_size: SampleSize::ReferToStream,
            coding_ext: 0,
            channel_allocation: 0,
            lfe_playback_level: LfePlaybackLevel::NoInfo,
            downmix_inhibit: false,
        });

        // Checksum: sum of all 31 bytes must be 0x00 mod 256.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(AudioWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        // PB1 (byte 4): bits[6:3] = CT, bits[2:0] = CC
        let pb1 = packet[4];
        if pb1 & 0x80 != 0 {
            decoded.push_warning(AudioWarning::ReservedFieldNonZero { byte: 4, bit: 7 });
        }
        let ct = (pb1 >> 3) & 0x0F;
        let cc = pb1 & 0x07;

        decoded.value.coding_type = match ct {
            0 => AudioCodingType::ReferToStream,
            1 => AudioCodingType::Lpcm,
            2 => AudioCodingType::Ac3,
            3 => AudioCodingType::Mpeg1,
            4 => AudioCodingType::Mp3,
            5 => AudioCodingType::Mpeg2Multichannel,
            6 => AudioCodingType::AacLc,
            7 => AudioCodingType::Dts,
            8 => AudioCodingType::Atrac,
            9 => AudioCodingType::OneBitAudio,
            10 => AudioCodingType::EnhancedAc3,
            11 => AudioCodingType::DtsHd,
            12 => AudioCodingType::MlpTrueHd,
            13 => AudioCodingType::Dst,
            14 => AudioCodingType::WmaPro,
            15 => AudioCodingType::Extension,
            _ => unreachable!(), // 4-bit field
        };

        decoded.value.channel_count = match cc {
            0 => ChannelCount::ReferToStream,
            n => ChannelCount::Count(n + 1), // wire CC = channels - 1
        };

        // PB2 (byte 5): bits[6:5] reserved, bits[4:2] = SF, bits[1:0] = SS
        let pb2 = packet[5];
        for bit in [6u8, 7] {
            if pb2 & (1 << bit) != 0 {
                decoded.push_warning(AudioWarning::ReservedFieldNonZero { byte: 5, bit });
            }
        }
        let sf = (pb2 >> 2) & 0x07;
        let ss = pb2 & 0x03;

        decoded.value.sample_freq = match sf {
            0 => SampleFrequency::ReferToStream,
            1 => SampleFrequency::Hz32000,
            2 => SampleFrequency::Hz44100,
            3 => SampleFrequency::Hz48000,
            4 => SampleFrequency::Hz88200,
            5 => SampleFrequency::Hz96000,
            6 => SampleFrequency::Hz176400,
            7 => SampleFrequency::Hz192000,
            _ => unreachable!(), // 3-bit field
        };

        decoded.value.sample_size = match ss {
            0 => SampleSize::ReferToStream,
            1 => SampleSize::Bits16,
            2 => SampleSize::Bits20,
            3 => SampleSize::Bits24,
            _ => unreachable!(), // 2-bit field
        };

        // PB3 (byte 6): bits[7:5] reserved, bits[4:0] = CXT
        let pb3 = packet[6];
        for bit in [5u8, 6, 7] {
            if pb3 & (1 << bit) != 0 {
                decoded.push_warning(AudioWarning::ReservedFieldNonZero { byte: 6, bit });
            }
        }
        decoded.value.coding_ext = pb3 & 0x1F;

        // PB4 (byte 7): channel allocation
        decoded.value.channel_allocation = packet[7];

        // PB5 (byte 8): bit[7] = DM_INH, bits[6:3] = LSV, bits[2:0] reserved
        let pb5 = packet[8];
        for bit in [0u8, 1, 2] {
            if pb5 & (1 << bit) != 0 {
                decoded.push_warning(AudioWarning::ReservedFieldNonZero { byte: 8, bit });
            }
        }
        decoded.value.downmix_inhibit = pb5 & 0x80 != 0;
        let lsv = (pb5 >> 3) & 0x0F;
        decoded.value.lfe_playback_level = match lsv {
            0 => LfePlaybackLevel::NoInfo,
            1 => LfePlaybackLevel::Plus10Db,
            2 => LfePlaybackLevel::Ref0Db,
            _ => {
                decoded.push_warning(AudioWarning::UnknownEnumValue {
                    field: "lfe_playback_level",
                    raw: lsv,
                });
                LfePlaybackLevel::NoInfo
            }
        };

        Ok(decoded)
    }
}

impl IntoPackets for AudioInfoFrame {
    type Iter = SinglePacketIter;
    type Warning = AudioWarning;

    fn into_packets(self) -> crate::decoded::Decoded<SinglePacketIter, AudioWarning> {
        let ct: u8 = match self.coding_type {
            AudioCodingType::ReferToStream => 0,
            AudioCodingType::Lpcm => 1,
            AudioCodingType::Ac3 => 2,
            AudioCodingType::Mpeg1 => 3,
            AudioCodingType::Mp3 => 4,
            AudioCodingType::Mpeg2Multichannel => 5,
            AudioCodingType::AacLc => 6,
            AudioCodingType::Dts => 7,
            AudioCodingType::Atrac => 8,
            AudioCodingType::OneBitAudio => 9,
            AudioCodingType::EnhancedAc3 => 10,
            AudioCodingType::DtsHd => 11,
            AudioCodingType::MlpTrueHd => 12,
            AudioCodingType::Dst => 13,
            AudioCodingType::WmaPro => 14,
            AudioCodingType::Extension => 15,
        };

        let cc: u8 = match self.channel_count {
            ChannelCount::ReferToStream => 0,
            // Actual channel count n (2–8) encodes as n-1 on the wire.
            ChannelCount::Count(n) => (n.saturating_sub(1)).min(7),
        };

        let sf: u8 = match self.sample_freq {
            SampleFrequency::ReferToStream => 0,
            SampleFrequency::Hz32000 => 1,
            SampleFrequency::Hz44100 => 2,
            SampleFrequency::Hz48000 => 3,
            SampleFrequency::Hz88200 => 4,
            SampleFrequency::Hz96000 => 5,
            SampleFrequency::Hz176400 => 6,
            SampleFrequency::Hz192000 => 7,
        };

        let ss: u8 = match self.sample_size {
            SampleSize::ReferToStream => 0,
            SampleSize::Bits16 => 1,
            SampleSize::Bits20 => 2,
            SampleSize::Bits24 => 3,
        };

        let lsv: u8 = match self.lfe_playback_level {
            LfePlaybackLevel::NoInfo => 0,
            LfePlaybackLevel::Plus10Db => 1,
            LfePlaybackLevel::Ref0Db => 2,
        };

        // Build the 30 bytes that feed into checksum computation:
        // [type, version, length, PB1..PB27].
        let mut hp = [0u8; 30];
        hp[0] = 0x84; // type code
        hp[1] = 0x01; // version
        hp[2] = 0x0A; // length = 10
        hp[3] = (ct << 3) | (cc & 0x07); // PB1
        hp[4] = (sf << 2) | (ss & 0x03); // PB2
        hp[5] = self.coding_ext & 0x1F; // PB3
        hp[6] = self.channel_allocation; // PB4
        hp[7] = ((self.downmix_inhibit as u8) << 7) | (lsv << 3); // PB5
        // PB6–PB10 (hp[8]–hp[12]) are reserved; already zero.

        let checksum = crate::checksum::compute_checksum(&hp);

        let mut packet = [0u8; 31];
        packet[..3].copy_from_slice(&hp[..3]); // type, version, length
        packet[3] = checksum;
        packet[4..].copy_from_slice(&hp[3..]); // PB1..PB27

        crate::decoded::Decoded::new(SinglePacketIter::new(packet))
    }
}

#[cfg(test)]
mod tests {
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
}
