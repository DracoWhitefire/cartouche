use crate::audio::AudioInfoFrame;
use crate::avi::AviInfoFrame;
use crate::dynamic_hdr::{DynamicHdrFragment, DynamicHdrInfoFrame};
use crate::encode::{IntoPackets, SinglePacketIter};
use crate::hdmi_forum_vsi::HdmiForumVsi;
use crate::hdr_static::HdrStaticInfoFrame;

/// An InfoFrame ready for encoding.
///
/// The encode-path top-level type. Holds a fully assembled InfoFrame of any
/// known type, or raw bytes for an unknown type code. Implements
/// [`IntoPackets`] to yield the wire packet
/// sequence for transmission.
///
/// For Dynamic HDR the variant holds a complete [`DynamicHdrInfoFrame`] whose
/// payload may span multiple packets. For all other types a single 31-byte
/// packet is produced.
///
/// The `Unknown` variant preserves every byte of the payload unmodified,
/// allowing round-trip handling of type codes not recognised by this version
/// of `cartouche`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InfoFrame {
    /// An AVI InfoFrame.
    Avi(AviInfoFrame),
    /// An Audio InfoFrame.
    Audio(AudioInfoFrame),
    /// An HDR Static Metadata InfoFrame.
    HdrStatic(HdrStaticInfoFrame),
    /// An HDMI Forum Vendor-Specific InfoFrame.
    HdmiForumVsi(HdmiForumVsi),
    /// A Dynamic HDR InfoFrame (multi-packet).
    DynamicHdr(DynamicHdrInfoFrame),
    /// A packet with an unrecognised type code.
    ///
    /// The `payload` field carries all 27 potential payload bytes verbatim.
    /// The checksum is recomputed on encode; the stored bytes are not assumed
    /// to be valid.
    Unknown {
        /// The type code byte from the packet header.
        type_code: u8,
        /// The version byte from the packet header.
        version: u8,
        /// The raw payload bytes (up to 27).
        payload: [u8; 27],
    },
}

/// The result of decoding a single wire packet without prior knowledge of its
/// type.
///
/// The decode-path top-level type, returned by [`decode`](crate::decode).
/// For all single-packet InfoFrame types the variant holds the fully decoded
/// frame. For Dynamic HDR — whose payload spans multiple packets — the variant
/// holds a [`DynamicHdrFragment`] carrying this packet's contribution to the
/// sequence.
///
/// Once a complete sequence of Dynamic HDR fragments has been collected, pass
/// the raw packets to `DynamicHdrInfoFrame::decode_sequence` to assemble the
/// full frame.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InfoFramePacket {
    /// A decoded AVI InfoFrame.
    Avi(AviInfoFrame),
    /// A decoded Audio InfoFrame.
    Audio(AudioInfoFrame),
    /// A decoded HDR Static Metadata InfoFrame.
    HdrStatic(HdrStaticInfoFrame),
    /// A decoded HDMI Forum Vendor-Specific InfoFrame.
    HdmiForumVsi(HdmiForumVsi),
    /// One packet from a Dynamic HDR sequence.
    DynamicHdrFragment(DynamicHdrFragment),
    /// A packet with an unrecognised type code.
    ///
    /// The `payload` field carries all 27 potential payload bytes verbatim.
    Unknown {
        /// The type code byte from the packet header.
        type_code: u8,
        /// The version byte from the packet header.
        version: u8,
        /// The raw payload bytes (up to 27).
        payload: [u8; 27],
    },
}

/// Iterator returned by [`IntoPackets`] for [`InfoFrame`].
///
/// Yields a single 31-byte packet for all traditional InfoFrame types.
/// The Dynamic HDR variant is not yet implemented (Phase 3) and yields no packets.
pub struct InfoFrameIter(Option<SinglePacketIter>);

impl Iterator for InfoFrameIter {
    type Item = [u8; 31];

    fn next(&mut self) -> Option<[u8; 31]> {
        self.0.as_mut()?.next()
    }
}

impl IntoPackets for InfoFrame {
    type Iter = InfoFrameIter;

    fn into_packets(self) -> InfoFrameIter {
        let iter = match self {
            InfoFrame::Avi(f) => Some(f.into_packets()),
            InfoFrame::Audio(f) => Some(f.into_packets()),
            InfoFrame::HdrStatic(f) => Some(f.into_packets()),
            InfoFrame::HdmiForumVsi(f) => Some(f.into_packets()),
            InfoFrame::DynamicHdr(_) => None, // Phase 3
            InfoFrame::Unknown {
                type_code,
                version,
                payload,
            } => {
                let mut hp = [0u8; 30];
                hp[0] = type_code;
                hp[1] = version;
                hp[2] = 27;
                hp[3..30].copy_from_slice(&payload);
                let checksum = crate::checksum::compute_checksum(&hp);
                let mut packet = [0u8; 31];
                packet[..3].copy_from_slice(&hp[..3]);
                packet[3] = checksum;
                packet[4..].copy_from_slice(&hp[3..]);
                Some(SinglePacketIter::new(packet))
            }
        };
        InfoFrameIter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{
        AudioCodingType, AudioInfoFrame, ChannelCount, LfePlaybackLevel, SampleFrequency,
        SampleSize,
    };
    use crate::avi::{
        AviInfoFrame, BarInfo, Colorimetry, ExtendedColorimetry, ItContentType, NonUniformScaling,
        PictureAspectRatio, RgbQuantization, ScanInfo, YccQuantization,
    };
    use crate::dynamic_hdr::DynamicHdrInfoFrame;
    use crate::hdmi_forum_vsi::HdmiForumVsi;
    use crate::hdr_static::{Eotf, HdrStaticInfoFrame, StaticMetadata, StaticMetadataType1};
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
        let mut iter = frame.into_packets();
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
        let mut iter = frame.into_packets();
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
        let mut iter = frame.into_packets();
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
        let mut iter = frame.into_packets();
        let packet = iter.next().expect("expected one packet");
        assert_eq!(packet[0], 0x81); // VSIF type code
        assert!(iter.next().is_none());
    }

    #[test]
    fn dynamic_hdr_variant_yields_no_packets() {
        let frame = InfoFrame::DynamicHdr(DynamicHdrInfoFrame {});
        assert!(frame.into_packets().next().is_none());
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
        let mut iter = frame.into_packets();
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
        let packet = frame.into_packets().next().unwrap();
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        assert_eq!(sum, 0, "checksum must make all-bytes sum equal 0");
    }
}
