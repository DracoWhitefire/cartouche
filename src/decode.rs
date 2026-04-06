use crate::audio::AudioInfoFrame;
use crate::avi::AviInfoFrame;
use crate::decoded::Decoded;
use crate::error::DecodeError;
use crate::frame::InfoFramePacket;
use crate::hdmi_forum_vsi::{HdmiForumVsi, HDMI_FORUM_OUI};
use crate::hdr_static::HdrStaticInfoFrame;
use crate::warn::Warning;

/// Type code constants for known InfoFrame types.
///
/// These are the values of byte 0 in the 31-byte wire packet header.
pub(crate) mod type_code {
    /// Vendor-Specific InfoFrame (includes HDMI Forum VSI).
    pub(crate) const VSIF: u8 = 0x81;
    /// AVI InfoFrame.
    pub(crate) const AVI: u8 = 0x82;
    /// Audio InfoFrame.
    pub(crate) const AUDIO: u8 = 0x84;
    /// HDR Static Metadata InfoFrame.
    pub(crate) const HDR_STATIC: u8 = 0x87;
    /// Dynamic HDR InfoFrame.
    pub(crate) const DYNAMIC_HDR: u8 = 0x20;
}

/// Decode a single 31-byte wire packet into an [`InfoFramePacket`].
///
/// This is the top-level decode entry point. It dispatches on the type code
/// in byte 0 of the packet and returns the appropriate [`InfoFramePacket`]
/// variant.
///
/// # Wire-level checks
///
/// - **Length**: byte 2 (`length`) declares the number of payload bytes. If
///   `length > 27` (the maximum payload capacity of a 31-byte packet) the
///   packet cannot be decoded and [`DecodeError::Truncated`] is returned.
/// - **Checksum**: verified by each per-type decoder; a mismatch is recorded
///   as a [`Warning`] on the returned result without preventing decode.
///
/// # Unknown type codes
///
/// Packets whose type code does not match any known InfoFrame type decode to
/// [`InfoFramePacket::Unknown`], with the type code, version, and raw payload
/// bytes preserved.
///
/// # VSIF dispatch
///
/// VSIF packets (type code 0x81) are further dispatched on the OUI in
/// PB1–PB3. Packets carrying the HDMI Forum OUI (0xD8, 0x5D, 0xC4) decode
/// to [`InfoFramePacket::HdmiForumVsi`]; all others fall through to
/// `InfoFramePacket::Unknown`.
///
/// # Errors
///
/// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
pub fn decode(packet: &[u8; 31]) -> Result<Decoded<InfoFramePacket, Warning>, DecodeError> {
    let length = packet[2];
    if length > 27 {
        return Err(DecodeError::Truncated {
            claimed: length,
            available: 27,
        });
    }

    let type_code = packet[0];
    let version = packet[1];
    let mut payload = [0u8; 27];
    payload.copy_from_slice(&packet[4..31]);

    match type_code {
        type_code::AVI => Ok(AviInfoFrame::decode(packet)?
            .wrap(InfoFramePacket::Avi, Warning::Avi)),
        type_code::AUDIO => Ok(AudioInfoFrame::decode(packet)?
            .wrap(InfoFramePacket::Audio, Warning::Audio)),
        type_code::HDR_STATIC => Ok(HdrStaticInfoFrame::decode(packet)?
            .wrap(InfoFramePacket::HdrStatic, Warning::HdrStatic)),
        type_code::VSIF => {
            if packet[4..7] == HDMI_FORUM_OUI {
                Ok(HdmiForumVsi::decode(packet)?
                    .wrap(InfoFramePacket::HdmiForumVsi, Warning::HdmiForumVsi))
            } else {
                Ok(Decoded::new(InfoFramePacket::Unknown { type_code, version, payload }))
            }
        }
        type_code::DYNAMIC_HDR => {
            // TODO(phase3): DynamicHdrFragment::decode(packet)
            Ok(Decoded::new(InfoFramePacket::Unknown { type_code, version, payload }))
        }
        _ => Ok(Decoded::new(InfoFramePacket::Unknown { type_code, version, payload })),
    }
}
