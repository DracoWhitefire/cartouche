use display_types::HdmiForumFrl;
use display_types::cea861::hdmi_forum::HdmiDscMaxSlices;

use crate::decoded::Decoded;
use crate::encode::{IntoPackets, SinglePacketIter};
use crate::error::DecodeError;
use crate::warn::HdmiForumVsiWarning;

/// HDMI Forum OUI transmitted in PB1–PB3 of every HDMI Forum VSIF.
///
/// Byte order as it appears on the wire: PB1 = 0xD8, PB2 = 0x5D, PB3 = 0xC4.
pub(crate) const HDMI_FORUM_OUI: [u8; 3] = [0xD8, 0x5D, 0xC4];

/// An HDMI Forum Vendor-Specific InfoFrame (HDMI 2.1 §10.2.5, type code 0x81).
///
/// Carries HDMI 2.1 auxiliary signaling: ALLM, VRR, DSC, QMS, and FRL rate.
/// Identified on the wire by the HDMI Forum OUI (0xD8, 0x5D, 0xC4) in
/// PB1–PB3.
///
/// # Bit layout reference (HDMI 2.1 §10.2.5)
///
/// - PB4: `allm`\[7\], `frl_rate`\[6:4\], `fapa_start_location`\[3\], `fva`\[2\]
/// - PB5: `vrr_en`\[7\], `m_const`\[6\], `qms_en`\[5\], `neg_mvrr`\[4\], `m_vrr`\[11:8\] in \[3:0\]
/// - PB6: `m_vrr`\[7:0\]
/// - PB7: `dsc_1p2`\[7\], `dsc_native_420`\[6\], `dsc_all_bpc`\[5\], `dsc_max_frl_rate`\[2:0\]
/// - PB8: `dsc_12bpc`\[7\], `dsc_10bpc`\[6\], `dsc_max_slices`\[3:0\]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HdmiForumVsi {
    // ---- PB4: general flags ----
    /// Auto Low Latency Mode (ALLM). Requests the sink to activate its lowest
    /// latency rendering pipeline.
    pub allm: bool,
    /// Current Fixed Rate Link (FRL) rate in use on this link.
    pub frl_rate: HdmiForumFrl,
    /// FAPA (Frame-Average Picture Activity) measurement start location.
    ///
    /// `false` = first horizontal blank of the frame; `true` = first
    /// horizontal blank following the first active video line.
    pub fapa_start_location: bool,
    /// Fast VActive (FVA). The source is using reduced vertical blanking.
    pub fva: bool,

    // ---- PB5–PB6: VRR / QMS ----
    /// Variable Refresh Rate enable.
    pub vrr_en: bool,
    /// M_VRR constant — the source is transmitting at a fixed M_VRR value.
    pub m_const: bool,
    /// Quick Media Switching (QMS) enable.
    pub qms_en: bool,
    /// Negative M_VRR values are in use.
    pub neg_mvrr: bool,
    /// M_VRR parameter (12-bit, values 0–4095).
    ///
    /// Meaningful only when `vrr_en` or `qms_en` is set.
    pub m_vrr: u16,

    // ---- PB7–PB8: DSC ----
    /// Source is using VESA DSC 1.2a compressed video transport.
    pub dsc_1p2: bool,
    /// DSC stream uses YCbCr 4:2:0 native pixel encoding.
    pub dsc_native_420: bool,
    /// DSC stream uses a non-standard bits-per-pixel value (`DSC_All_BPC`).
    pub dsc_all_bpc: bool,
    /// Maximum FRL rate the DSC-compressed stream requires.
    pub dsc_max_frl_rate: HdmiForumFrl,
    /// Maximum number of horizontal DSC slices used by the stream.
    pub dsc_max_slices: HdmiDscMaxSlices,
    /// DSC stream uses 10 bpc compressed video.
    pub dsc_10bpc: bool,
    /// DSC stream uses 12 bpc compressed video.
    pub dsc_12bpc: bool,
}

impl HdmiForumVsi {
    /// Decode an HDMI Forum VSIF from a 31-byte wire packet.
    ///
    /// The caller is responsible for confirming that the packet's type code is
    /// 0x81 (VSIF) and that PB1–PB3 contain the HDMI Forum OUI before calling
    /// this function. The top-level [`decode`](crate::decode) dispatch handles
    /// this routing automatically.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    pub fn decode(
        packet: &[u8; 31],
    ) -> Result<Decoded<HdmiForumVsi, HdmiForumVsiWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated {
                claimed: length,
                available: 27,
            });
        }

        let mut decoded = Decoded::new(HdmiForumVsi {
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

        // Checksum.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(HdmiForumVsiWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        // PB4 (byte 7): ALLM[7], FRL_Rate[6:4], FAPA_Start_Location[3], FVA[2], rsvd[1:0]
        let pb4 = packet[7];
        for bit in [0u8, 1] {
            if pb4 & (1 << bit) != 0 {
                decoded.push_warning(HdmiForumVsiWarning::ReservedFieldNonZero { byte: 7, bit });
            }
        }
        decoded.value.allm = pb4 & 0x80 != 0;
        decoded.value.fapa_start_location = pb4 & 0x08 != 0;
        decoded.value.fva = pb4 & 0x04 != 0;

        let frl_raw = (pb4 >> 4) & 0x07;
        if frl_raw > 6 {
            decoded.push_warning(HdmiForumVsiWarning::UnknownEnumValue {
                field: "frl_rate",
                raw: frl_raw,
            });
        }
        decoded.value.frl_rate = HdmiForumFrl::from_raw(frl_raw);

        // PB5 (byte 8): VRR_En[7], M_CONST[6], QMS_En[5], Neg_MVRR[4], M_VRR[11:8] in [3:0]
        let pb5 = packet[8];
        decoded.value.vrr_en = pb5 & 0x80 != 0;
        decoded.value.m_const = pb5 & 0x40 != 0;
        decoded.value.qms_en = pb5 & 0x20 != 0;
        decoded.value.neg_mvrr = pb5 & 0x10 != 0;

        // PB6 (byte 9): M_VRR[7:0]
        let m_vrr_hi = (pb5 & 0x0F) as u16;
        let m_vrr_lo = packet[9] as u16;
        decoded.value.m_vrr = (m_vrr_hi << 8) | m_vrr_lo;

        // PB7 (byte 10): DSC_1p2[7], DSC_Native_420[6], DSC_All_BPC[5], rsvd[4:3], DSC_Max_FRL_Rate[2:0]
        let pb7 = packet[10];
        for bit in [3u8, 4] {
            if pb7 & (1 << bit) != 0 {
                decoded.push_warning(HdmiForumVsiWarning::ReservedFieldNonZero { byte: 10, bit });
            }
        }
        decoded.value.dsc_1p2 = pb7 & 0x80 != 0;
        decoded.value.dsc_native_420 = pb7 & 0x40 != 0;
        decoded.value.dsc_all_bpc = pb7 & 0x20 != 0;

        let dsc_frl_raw = pb7 & 0x07;
        if dsc_frl_raw > 6 {
            decoded.push_warning(HdmiForumVsiWarning::UnknownEnumValue {
                field: "dsc_max_frl_rate",
                raw: dsc_frl_raw,
            });
        }
        decoded.value.dsc_max_frl_rate = HdmiForumFrl::from_raw(dsc_frl_raw);

        // PB8 (byte 11): DSC_12bpc[7], DSC_10bpc[6], rsvd[5:4], DSC_Max_Slices[3:0]
        let pb8 = packet[11];
        for bit in [4u8, 5] {
            if pb8 & (1 << bit) != 0 {
                decoded.push_warning(HdmiForumVsiWarning::ReservedFieldNonZero { byte: 11, bit });
            }
        }
        decoded.value.dsc_12bpc = pb8 & 0x80 != 0;
        decoded.value.dsc_10bpc = pb8 & 0x40 != 0;

        let slices_raw = pb8 & 0x0F;
        if slices_raw > 7 {
            decoded.push_warning(HdmiForumVsiWarning::UnknownEnumValue {
                field: "dsc_max_slices",
                raw: slices_raw,
            });
        }
        decoded.value.dsc_max_slices = HdmiDscMaxSlices::from_raw(slices_raw);

        Ok(decoded)
    }
}

impl IntoPackets for HdmiForumVsi {
    type Iter = SinglePacketIter;

    fn into_packets(self) -> SinglePacketIter {
        let mut hp = [0u8; 30];
        hp[0] = 0x81; // type code (VSIF)
        hp[1] = 0x01; // version
        hp[2] = 8; // length: PB1–PB8

        // PB1–PB3: HDMI Forum OUI
        hp[3] = HDMI_FORUM_OUI[0];
        hp[4] = HDMI_FORUM_OUI[1];
        hp[5] = HDMI_FORUM_OUI[2];

        // PB4: ALLM[7], FRL_Rate[6:4], FAPA_Start_Location[3], FVA[2]
        let frl_raw = self.frl_rate as u8;
        hp[6] = ((self.allm as u8) << 7)
            | ((frl_raw & 0x07) << 4)
            | ((self.fapa_start_location as u8) << 3)
            | ((self.fva as u8) << 2);

        // PB5: VRR_En[7], M_CONST[6], QMS_En[5], Neg_MVRR[4], M_VRR[11:8] in [3:0]
        hp[7] = ((self.vrr_en as u8) << 7)
            | ((self.m_const as u8) << 6)
            | ((self.qms_en as u8) << 5)
            | ((self.neg_mvrr as u8) << 4)
            | (((self.m_vrr >> 8) & 0x0F) as u8);

        // PB6: M_VRR[7:0]
        hp[8] = (self.m_vrr & 0xFF) as u8;

        // PB7: DSC_1p2[7], DSC_Native_420[6], DSC_All_BPC[5], DSC_Max_FRL_Rate[2:0]
        let dsc_frl_raw = self.dsc_max_frl_rate as u8;
        hp[9] = ((self.dsc_1p2 as u8) << 7)
            | ((self.dsc_native_420 as u8) << 6)
            | ((self.dsc_all_bpc as u8) << 5)
            | (dsc_frl_raw & 0x07);

        // PB8: DSC_12bpc[7], DSC_10bpc[6], DSC_Max_Slices[3:0]
        let slices_raw = self.dsc_max_slices as u8;
        hp[10] =
            ((self.dsc_12bpc as u8) << 7) | ((self.dsc_10bpc as u8) << 6) | (slices_raw & 0x0F);

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

    fn full_frame() -> HdmiForumVsi {
        HdmiForumVsi {
            allm: true,
            frl_rate: HdmiForumFrl::Rate10Gbps4Lanes,
            fapa_start_location: true,
            fva: false,
            vrr_en: true,
            m_const: false,
            qms_en: true,
            neg_mvrr: false,
            m_vrr: 0x0ABC,
            dsc_1p2: true,
            dsc_native_420: false,
            dsc_all_bpc: true,
            dsc_max_frl_rate: HdmiForumFrl::Rate6Gbps4Lanes,
            dsc_max_slices: HdmiDscMaxSlices::Slices8At400Mhz,
            dsc_10bpc: true,
            dsc_12bpc: false,
        }
    }

    #[test]
    fn round_trip() {
        let frame = full_frame();
        let packet = frame.clone().into_packets().next().unwrap();
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value, frame);
    }

    #[test]
    fn oui_in_correct_positions() {
        let packet = full_frame().into_packets().next().unwrap();
        assert_eq!(&packet[4..7], &HDMI_FORUM_OUI);
    }

    #[test]
    fn checksum_mismatch_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[3] = packet[3].wrapping_add(1);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(
            decoded
                .iter_warnings()
                .any(|w| matches!(w, HdmiForumVsiWarning::ChecksumMismatch { .. }))
        );
        assert_eq!(decoded.value, full_frame());
    }

    #[test]
    fn truncated_length_is_error() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[2] = 28;
        assert!(matches!(
            HdmiForumVsi::decode(&packet),
            Err(DecodeError::Truncated {
                claimed: 28,
                available: 27
            })
        ));
    }

    #[test]
    fn unknown_frl_rate_warns_and_decodes() {
        let mut packet = full_frame().into_packets().next().unwrap();
        // frl_rate occupies PB4 bits [6:4]; set to 0x07 (7) which is out of spec
        packet[7] = (packet[7] & 0x8F) | (0x07 << 4);
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::UnknownEnumValue {
                field: "frl_rate",
                raw: 7
            }
        )));
    }

    #[test]
    fn reserved_bits_pb7_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[10] |= 0x08; // set reserved bit 3 of PB7 (byte 10)
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::ReservedFieldNonZero { byte: 10, bit: 3 }
        )));
    }

    #[test]
    fn reserved_bits_pb8_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[11] |= 0x10; // set reserved bit 4 of PB8 (byte 11)
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::ReservedFieldNonZero { byte: 11, bit: 4 }
        )));
    }

    #[test]
    fn unknown_dsc_max_frl_rate_warns() {
        let mut packet = full_frame().into_packets().next().unwrap();
        // dsc_max_frl_rate occupies PB7 bits [2:0]; set to 7 (out of spec)
        packet[10] = (packet[10] & 0xF8) | 0x07;
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::UnknownEnumValue {
                field: "dsc_max_frl_rate",
                raw: 7
            }
        )));
    }

    #[test]
    fn unknown_dsc_max_slices_warns() {
        let mut packet = full_frame().into_packets().next().unwrap();
        // dsc_max_slices occupies PB8 bits [3:0]; set to 0x0F (out of spec)
        packet[11] = (packet[11] & 0xF0) | 0x0F;
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::UnknownEnumValue {
                field: "dsc_max_slices",
                raw: 15
            }
        )));
    }

    #[test]
    fn reserved_bit_warning() {
        let mut packet = full_frame().into_packets().next().unwrap();
        packet[7] |= 0x01; // set reserved bit 0 of PB4
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = HdmiForumVsi::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            HdmiForumVsiWarning::ReservedFieldNonZero { byte: 7, bit: 0 }
        )));
    }
}
