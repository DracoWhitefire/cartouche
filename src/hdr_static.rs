/// Electro-Optical Transfer Function (EOTF field, PB1 bits 2–0).
///
/// Identifies the transfer function applied to the content. This governs how
/// the sink maps signal values to display luminance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Eotf {
    /// Traditional gamma, SDR luminance range (EOTF = 0).
    TraditionalGammaSdr,
    /// Traditional gamma, HDR luminance range (EOTF = 1).
    TraditionalGammaHdr,
    /// SMPTE ST 2084 (Perceptual Quantizer / PQ), used for HDR10 (EOTF = 2).
    Pq,
    /// Hybrid Log-Gamma (HLG), ITU-R BT.2100 (EOTF = 3).
    Hlg,
}

/// Type 1 static metadata (SMPTE ST 2086 mastering display metadata).
///
/// Carries the chromaticity coordinates of the mastering display primaries and
/// white point, the mastering display luminance range, and the content light
/// level metadata (MaxCLL / MaxFALL).
///
/// All chromaticity values are in units of 0.00002 (i.e. divide by 50 000 to
/// get CIE 1931 xy). Mastering luminance values are in cd/m²; MaxCLL and
/// MaxFALL are in cd/m².
///
/// Primary order follows CTA-861 / SMPTE ST 2086: green first, then blue,
/// then red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticMetadataType1 {
    /// Mastering display green primary chromaticity (x, y) × 50 000.
    pub primaries_green: [u16; 2],
    /// Mastering display blue primary chromaticity (x, y) × 50 000.
    pub primaries_blue: [u16; 2],
    /// Mastering display red primary chromaticity (x, y) × 50 000.
    pub primaries_red: [u16; 2],
    /// Mastering display white point (x, y) × 50 000.
    pub white_point: [u16; 2],
    /// Maximum mastering display luminance in cd/m².
    pub max_mastering_luminance: u16,
    /// Minimum mastering display luminance in cd/m².
    pub min_mastering_luminance: u16,
    /// Maximum Content Light Level (MaxCLL) in cd/m².
    pub max_cll: u16,
    /// Maximum Frame-Average Light Level (MaxFALL) in cd/m².
    pub max_fall: u16,
}

/// Static metadata payload, selected by the descriptor ID field (PB1 bits 5–3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StaticMetadata {
    /// Type 1 descriptor (ID = 0): SMPTE ST 2086 + MaxCLL/MaxFALL.
    Type1(StaticMetadataType1),
    /// An unrecognised descriptor ID.
    ///
    /// `data` carries the raw payload bytes PB2–PB27 verbatim.
    Unknown {
        /// The raw descriptor ID value from PB1 bits 5–3.
        descriptor_id: u8,
        /// Raw payload bytes following the EOTF/descriptor byte (PB2–PB27).
        data: [u8; 26],
    },
}

/// An HDR Static Metadata InfoFrame (CTA-861, type code 0x87).
///
/// Carries the EOTF and the static HDR metadata for the content being
/// transmitted. Required for HDR10 and HLG content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HdrStaticInfoFrame {
    /// Electro-optical transfer function (EOTF, PB1 bits 2–0).
    pub eotf: Eotf,
    /// Static metadata payload, identified by the descriptor ID in PB1 bits 5–3.
    pub metadata: StaticMetadata,
}

use crate::decoded::Decoded;
use crate::encode::{IntoPackets, SinglePacketIter};
use crate::error::DecodeError;
use crate::warn::HdrStaticWarning;

impl HdrStaticInfoFrame {
    /// Decode an HDR Static Metadata InfoFrame from a 31-byte wire packet.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry any of:
    /// - [`HdrStaticWarning::ChecksumMismatch`]
    /// - [`HdrStaticWarning::ReservedFieldNonZero`]
    /// - [`HdrStaticWarning::UnknownEnumValue`]
    pub fn decode(
        packet: &[u8; 31],
    ) -> Result<Decoded<HdrStaticInfoFrame, HdrStaticWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated {
                claimed: length,
                available: 27,
            });
        }

        let mut decoded = Decoded::new(HdrStaticInfoFrame {
            eotf: Eotf::TraditionalGammaSdr,
            metadata: StaticMetadata::Unknown {
                descriptor_id: 0,
                data: [0u8; 26],
            },
        });

        // Checksum verification.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(HdrStaticWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        // PB1 (byte 4): bits[7:6] reserved, bits[5:3] = descriptor ID, bits[2:0] = EOTF
        let pb1 = packet[4];
        for bit in [6u8, 7] {
            if pb1 & (1 << bit) != 0 {
                decoded.push_warning(HdrStaticWarning::ReservedFieldNonZero { byte: 4, bit });
            }
        }
        let eotf_raw = pb1 & 0x07;
        let descriptor_id = (pb1 >> 3) & 0x07;

        decoded.value.eotf = match eotf_raw {
            0 => Eotf::TraditionalGammaSdr,
            1 => Eotf::TraditionalGammaHdr,
            2 => Eotf::Pq,
            3 => Eotf::Hlg,
            _ => {
                decoded.push_warning(HdrStaticWarning::UnknownEnumValue {
                    field: "eotf",
                    raw: eotf_raw,
                });
                Eotf::TraditionalGammaSdr
            }
        };

        decoded.value.metadata = match descriptor_id {
            0 => {
                let md = StaticMetadataType1 {
                    primaries_green: [
                        u16::from_le_bytes([packet[5], packet[6]]),
                        u16::from_le_bytes([packet[7], packet[8]]),
                    ],
                    primaries_blue: [
                        u16::from_le_bytes([packet[9], packet[10]]),
                        u16::from_le_bytes([packet[11], packet[12]]),
                    ],
                    primaries_red: [
                        u16::from_le_bytes([packet[13], packet[14]]),
                        u16::from_le_bytes([packet[15], packet[16]]),
                    ],
                    white_point: [
                        u16::from_le_bytes([packet[17], packet[18]]),
                        u16::from_le_bytes([packet[19], packet[20]]),
                    ],
                    max_mastering_luminance: u16::from_le_bytes([packet[21], packet[22]]),
                    min_mastering_luminance: u16::from_le_bytes([packet[23], packet[24]]),
                    max_cll: u16::from_le_bytes([packet[25], packet[26]]),
                    max_fall: u16::from_le_bytes([packet[27], packet[28]]),
                };
                StaticMetadata::Type1(md)
            }
            id => {
                decoded.push_warning(HdrStaticWarning::UnknownEnumValue {
                    field: "static_metadata_descriptor_id",
                    raw: id,
                });
                let mut data = [0u8; 26];
                data.copy_from_slice(&packet[5..31]);
                StaticMetadata::Unknown {
                    descriptor_id: id,
                    data,
                }
            }
        };

        Ok(decoded)
    }
}

impl IntoPackets for HdrStaticInfoFrame {
    type Iter = SinglePacketIter;

    fn into_packets(self) -> SinglePacketIter {
        let eotf_raw: u8 = match self.eotf {
            Eotf::TraditionalGammaSdr => 0,
            Eotf::TraditionalGammaHdr => 1,
            Eotf::Pq => 2,
            Eotf::Hlg => 3,
        };

        let mut hp = [0u8; 30];
        hp[0] = 0x87; // type code
        hp[1] = 0x01; // version
        // length: PB1 through PB25 = 25 bytes for Type 1; PB1 + 26 raw bytes for Unknown
        // We always encode 26 payload bytes (PB1–PB26) to keep a fixed-length packet.
        hp[2] = 26;

        match self.metadata {
            StaticMetadata::Type1(md) => {
                hp[3] = eotf_raw & 0x07; // descriptor_id = 0, so no shift needed

                let write_u16 = |buf: &mut [u8; 30], offset: usize, val: u16| {
                    let [lo, hi] = val.to_le_bytes();
                    buf[offset] = lo;
                    buf[offset + 1] = hi;
                };

                write_u16(&mut hp, 4, md.primaries_green[0]);
                write_u16(&mut hp, 6, md.primaries_green[1]);
                write_u16(&mut hp, 8, md.primaries_blue[0]);
                write_u16(&mut hp, 10, md.primaries_blue[1]);
                write_u16(&mut hp, 12, md.primaries_red[0]);
                write_u16(&mut hp, 14, md.primaries_red[1]);
                write_u16(&mut hp, 16, md.white_point[0]);
                write_u16(&mut hp, 18, md.white_point[1]);
                write_u16(&mut hp, 20, md.max_mastering_luminance);
                write_u16(&mut hp, 22, md.min_mastering_luminance);
                write_u16(&mut hp, 24, md.max_cll);
                write_u16(&mut hp, 26, md.max_fall);
                // hp[28..29] = PB26 reserved, already zero.
            }
            StaticMetadata::Unknown {
                descriptor_id,
                data,
            } => {
                hp[3] = ((descriptor_id & 0x07) << 3) | (eotf_raw & 0x07);
                hp[4..30].copy_from_slice(&data);
            }
        }

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
        let packet = frame.clone().into_packets().next().unwrap();
        let decoded = HdrStaticInfoFrame::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value, frame);
    }

    #[test]
    fn checksum_mismatch_warning() {
        let mut packet = type1_frame().into_packets().next().unwrap();
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
        let mut packet = type1_frame().into_packets().next().unwrap();
        packet[2] = 28;
        assert!(matches!(
            HdrStaticInfoFrame::decode(&packet),
            Err(DecodeError::Truncated {
                claimed: 28,
                available: 27
            })
        ));
    }

    #[test]
    fn unknown_eotf_warns_and_falls_back() {
        let mut packet = type1_frame().into_packets().next().unwrap();
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
            let packet = frame.clone().into_packets().next().unwrap();
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
        let packet = frame.clone().into_packets().next().unwrap();
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
        let mut packet = type1_frame().into_packets().next().unwrap();
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
        let mut packet = type1_frame().into_packets().next().unwrap();
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
}
