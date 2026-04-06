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
            InfoFrame::Avi(f)          => Some(f.into_packets()),
            InfoFrame::Audio(f)        => Some(f.into_packets()),
            InfoFrame::HdrStatic(f)    => Some(f.into_packets()),
            InfoFrame::HdmiForumVsi(f) => Some(f.into_packets()),
            InfoFrame::DynamicHdr(_) => None, // Phase 3
            InfoFrame::Unknown { type_code, version, payload } => {
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
