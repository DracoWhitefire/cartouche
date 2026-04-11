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
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = HdmiForumVsi::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(decoded.value, frame);
}

#[test]
fn oui_in_correct_positions() {
    let packet = full_frame().into_packets().value.next().unwrap();
    assert_eq!(&packet[4..7], &HDMI_FORUM_OUI);
}

#[test]
fn checksum_mismatch_warning() {
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
    packet[2] = 28;
    assert!(matches!(
        HdmiForumVsi::decode(&packet),
        Err(DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn unknown_frl_rate_warns_and_decodes() {
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
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
    let mut packet = full_frame().into_packets().value.next().unwrap();
    packet[7] |= 0x01; // set reserved bit 0 of PB4
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = HdmiForumVsi::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        HdmiForumVsiWarning::ReservedFieldNonZero { byte: 7, bit: 0 }
    )));
}

#[cfg(feature = "serde")]
#[test]
fn hdmi_forum_vsi_round_trip() {
    use super::*;
    use display_types::HdmiForumFrl;
    use display_types::cea861::hdmi_forum::HdmiDscMaxSlices;
    let vsi = HdmiForumVsi {
        allm: true,
        frl_rate: HdmiForumFrl::Rate8Gbps4Lanes,
        fapa_start_location: true,
        fva: false,
        vrr_en: true,
        m_const: false,
        qms_en: true,
        neg_mvrr: false,
        m_vrr: 120,
        dsc_1p2: true,
        dsc_native_420: false,
        dsc_all_bpc: true,
        dsc_max_frl_rate: HdmiForumFrl::NotSupported,
        dsc_max_slices: HdmiDscMaxSlices::NotSupported,
        dsc_10bpc: true,
        dsc_12bpc: false,
    };
    let json = serde_json::to_string(&vsi).unwrap();
    let back: HdmiForumVsi = serde_json::from_str(&json).unwrap();
    assert_eq!(vsi, back);
}
