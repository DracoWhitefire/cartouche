use super::*;
use crate::encode::IntoPackets;

fn type1_frame() -> HdrStaticInfoFrame {
    HdrStaticInfoFrame {
        eotf: Eotf::Pq,
        metadata: StaticMetadata::Type1(StaticMetadataType1 {
            primaries_green: [0x3D13, 0x8000],
            primaries_blue: [0x1DB5, 0x0452],
            primaries_red: [0x84D0, 0x3D13],
            white_point: [0x3D13, 0x4042],
            max_mastering_luminance: 1000,
            min_mastering_luminance: 1,
            max_cll: 1000,
            max_fall: 400,
        }),
    }
}

#[test]
fn round_trip() {
    let frame = type1_frame();
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(decoded.value, frame);
}

#[test]
fn checksum_mismatch_warning() {
    let mut packet = type1_frame().into_packets().value.next().unwrap();
    packet[3] = packet[3].wrapping_add(1);
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, HdrStaticWarning::ChecksumMismatch { .. }))
    );
    assert_eq!(decoded.value, type1_frame());
}

#[test]
fn truncated_length_is_error() {
    let mut packet = type1_frame().into_packets().value.next().unwrap();
    packet[2] = 28;
    assert!(matches!(
        HdrStaticInfoFrame::decode(&packet),
        Err(DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn unknown_eotf_warns_and_falls_back() {
    let mut packet = type1_frame().into_packets().value.next().unwrap();
    packet[4] = (packet[4] & !0x07) | 0x07; // set EOTF to 7 (reserved)
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        HdrStaticWarning::UnknownEnumValue {
            field: "eotf",
            raw: 7
        }
    )));
    assert_eq!(decoded.value.eotf, Eotf::TraditionalGammaSdr);
}

#[test]
fn eotf_variants_round_trip() {
    for eotf in [
        Eotf::TraditionalGammaSdr,
        Eotf::TraditionalGammaHdr,
        Eotf::Hlg,
    ] {
        let frame = HdrStaticInfoFrame {
            eotf,
            ..type1_frame()
        };
        let packet = frame.clone().into_packets().value.next().unwrap();
        let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value.eotf, eotf);
    }
}

#[test]
fn unknown_metadata_round_trip() {
    let mut data = [0u8; 26];
    data[0] = 0xAB;
    data[25] = 0xCD;
    let frame = HdrStaticInfoFrame {
        eotf: Eotf::Pq,
        metadata: StaticMetadata::Unknown {
            descriptor_id: 3,
            data,
        },
    };
    let packet = frame.clone().into_packets().value.next().unwrap();
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        HdrStaticWarning::UnknownEnumValue {
            field: "static_metadata_descriptor_id",
            raw: 3
        }
    )));
    if let StaticMetadata::Unknown {
        descriptor_id,
        data: d,
    } = decoded.value.metadata
    {
        assert_eq!(descriptor_id, 3);
        assert_eq!(d[0], 0xAB);
        assert_eq!(d[25], 0xCD);
    } else {
        panic!("expected Unknown metadata variant");
    }
}

#[test]
fn reserved_pb1_bits_warning() {
    let mut packet = type1_frame().into_packets().value.next().unwrap();
    packet[4] |= 0x40; // set reserved bit 6 of PB1
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        HdrStaticWarning::ReservedFieldNonZero { byte: 4, bit: 6 }
    )));
}

#[test]
fn unknown_descriptor_id_warns() {
    let mut packet = type1_frame().into_packets().value.next().unwrap();
    packet[4] = (packet[4] & !0x38) | (0x05 << 3); // set descriptor_id to 5
    let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    packet[3] = packet[3].wrapping_sub(sum);
    let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        HdrStaticWarning::UnknownEnumValue {
            field: "static_metadata_descriptor_id",
            raw: 5
        }
    )));
    assert!(matches!(
        decoded.value.metadata,
        StaticMetadata::Unknown {
            descriptor_id: 5,
            ..
        }
    ));
}
