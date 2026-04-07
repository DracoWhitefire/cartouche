use crate::decoded::Decoded;
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;

/// A Dynamic HDR InfoFrame.
///
/// Carries per-frame or per-scene dynamic tone mapping metadata for formats
/// including HDR10+ (ETSI TS 103 433) and SL-HDR. Unlike all other InfoFrame
/// types, the payload is variable length and spans multiple packets.
///
/// Use [`DynamicHdrFragment::decode`] to decode individual packets as they
/// arrive. Once the full sequence is assembled, pass the raw packets to
/// [`DynamicHdrInfoFrame::decode_sequence`] to obtain this type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DynamicHdrInfoFrame {
    /// An unrecognised metadata format.
    ///
    /// Returned when the format identifier in the packet sequence is not
    /// recognised by this version of `cartouche`. The raw payload is not
    /// preserved; `format_id` identifies the format.
    ///
    /// Because the payload bytes are not retained, this variant cannot be
    /// re-encoded via [`IntoPackets`](crate::encode::IntoPackets).
    Unknown {
        /// The metadata format identifier from the first packet in the sequence.
        format_id: u8,
    },
}

impl DynamicHdrInfoFrame {
    /// Assemble a [`DynamicHdrInfoFrame`] from a complete sequence of wire packets.
    ///
    /// The caller is responsible for collecting the sequence. When the sum of
    /// `chunk_len` values across all [`DynamicHdrFragment`]s received via the
    /// top-level [`decode`](crate::decode) function equals `total_bytes`, the
    /// sequence is complete and ready to pass here.
    ///
    /// The format identifier is read from the first packet in the sequence
    /// (byte 7, PB3). Unknown format identifiers produce
    /// [`DynamicHdrInfoFrame::Unknown`]; the raw payload bytes are not retained.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if any packet in `packets` has
    /// `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`] — for any packet whose
    ///   checksum does not verify.
    pub fn decode_sequence(
        packets: &[[u8; 31]],
    ) -> Result<Decoded<DynamicHdrInfoFrame, DynamicHdrWarning>, DecodeError> {
        let mut decoded = Decoded::new(DynamicHdrInfoFrame::Unknown { format_id: 0 });

        for packet in packets {
            let length = packet[2];
            if length > 27 {
                return Err(DecodeError::Truncated { claimed: length });
            }

            let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
            if total != 0x00 {
                let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
                decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
                    expected,
                    found: packet[3],
                });
            }
        }

        if let Some(first) = packets.first() {
            // format_id lives at byte 7 (PB3) of every packet.
            let format_id = first[7];
            decoded.value = DynamicHdrInfoFrame::Unknown { format_id };
        }

        Ok(decoded)
    }
}

/// A single packet's worth of Dynamic HDR metadata, as returned by the
/// top-level [`decode`](crate::decode) function.
///
/// A full [`DynamicHdrInfoFrame`] cannot be assembled from a single wire
/// packet. The top-level decode path therefore returns this fragment type,
/// which exposes the fields the caller needs to accumulate a complete sequence.
/// Once all packets in the sequence have been collected, pass them to
/// `DynamicHdrInfoFrame::decode_sequence` to assemble the full frame.
///
/// # Wire layout
///
/// ```text
/// Byte 4 (PB0):    seq_num    — packet index in sequence (0-based)
/// Bytes 5–6 (PB1–2): total_bytes — total metadata byte count (little-endian u16)
/// Byte 7 (PB3):    format_id  — metadata format identifier
/// Bytes 8–30 (PB4–26): chunk  — up to 23 metadata bytes
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DynamicHdrFragment {
    /// Zero-indexed position of this packet in the sequence.
    pub seq_num: u8,
    /// Total metadata byte count declared in the packet header.
    ///
    /// The sequence is complete when the sum of `chunk_len` values across all
    /// received fragments reaches this value.
    pub total_bytes: u16,
    /// Identifies the metadata format (HDR10+, SL-HDR, etc.).
    ///
    /// Unrecognised format identifiers are preserved here; `Unknown` at the
    /// `InfoFramePacket` level is a type-code catch-all, not a format catch-all.
    pub format_id: u8,
    /// The metadata bytes carried by this packet.
    ///
    /// Only `chunk[..chunk_len as usize]` contains meaningful data. The final
    /// packet in a sequence may carry fewer than 23 bytes.
    pub chunk: [u8; 23],
    /// Number of valid bytes in [`chunk`](DynamicHdrFragment::chunk).
    ///
    /// Always ≤ 23.
    pub chunk_len: u8,
}

impl DynamicHdrFragment {
    /// Decode a single Dynamic HDR InfoFrame packet into a fragment.
    ///
    /// Each Dynamic HDR packet carries a sequence index, the total metadata
    /// byte count, a format identifier, and up to 23 bytes of metadata. The
    /// caller is responsible for collecting fragments until the sequence is
    /// complete, then passing the full packet sequence to
    /// `DynamicHdrInfoFrame::decode_sequence`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`]
    pub fn decode(
        packet: &[u8; 31],
    ) -> Result<Decoded<DynamicHdrFragment, DynamicHdrWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated { claimed: length });
        }

        let mut decoded = Decoded::new(DynamicHdrFragment {
            seq_num: 0,
            total_bytes: 0,
            format_id: 0,
            chunk: [0u8; 23],
            chunk_len: 0,
        });

        // Checksum verification.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        decoded.value.seq_num = packet[4];
        decoded.value.total_bytes = u16::from_le_bytes([packet[5], packet[6]]);
        decoded.value.format_id = packet[7];

        // chunk_len = payload bytes after the 4-byte overhead, capped at 23.
        let chunk_len = length.saturating_sub(4).min(23);
        decoded.value.chunk_len = chunk_len;
        decoded.value.chunk[..chunk_len as usize]
            .copy_from_slice(&packet[8..8 + chunk_len as usize]);

        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(seq_num: u8, total_bytes: u16, format_id: u8, chunk: &[u8]) -> [u8; 31] {
        let chunk_len = chunk.len().min(23) as u8;
        let length = 4 + chunk_len;
        let mut packet = [0u8; 31];
        packet[0] = 0x20; // Dynamic HDR type code
        packet[1] = 0x01; // version
        packet[2] = length;
        packet[4] = seq_num;
        packet[5] = (total_bytes & 0xFF) as u8;
        packet[6] = (total_bytes >> 8) as u8;
        packet[7] = format_id;
        packet[8..8 + chunk_len as usize].copy_from_slice(&chunk[..chunk_len as usize]);
        // Compute and insert checksum.
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        packet
    }

    #[test]
    fn round_trip_fields() {
        let chunk_data: [u8; 23] = core::array::from_fn(|i| i as u8);
        let packet = make_packet(3, 0x0142, 0x04, &chunk_data);
        let decoded = DynamicHdrFragment::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value.seq_num, 3);
        assert_eq!(decoded.value.total_bytes, 0x0142);
        assert_eq!(decoded.value.format_id, 0x04);
        assert_eq!(decoded.value.chunk_len, 23);
        assert_eq!(&decoded.value.chunk[..23], &chunk_data);
    }

    #[test]
    fn partial_chunk_last_packet() {
        // Simulate a final packet with only 5 metadata bytes.
        let chunk_data = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let packet = make_packet(1, 28, 0x04, &chunk_data);
        let decoded = DynamicHdrFragment::decode(&packet).unwrap();
        assert_eq!(decoded.value.chunk_len, 5);
        assert_eq!(&decoded.value.chunk[..5], &chunk_data);
    }

    #[test]
    fn checksum_mismatch_warning() {
        let mut packet = make_packet(0, 23, 0x04, &[0u8; 23]);
        packet[3] = packet[3].wrapping_add(1); // corrupt checksum
        let decoded = DynamicHdrFragment::decode(&packet).unwrap();
        assert!(
            decoded
                .iter_warnings()
                .any(|w| matches!(w, DynamicHdrWarning::ChecksumMismatch { .. }))
        );
    }

    #[test]
    fn truncated_length_is_error() {
        let mut packet = make_packet(0, 0, 0x00, &[]);
        packet[2] = 28; // > 27
        assert!(matches!(
            DynamicHdrFragment::decode(&packet),
            Err(DecodeError::Truncated { claimed: 28 })
        ));
    }

    #[test]
    fn zero_length_payload() {
        // Degenerate packet with length = 0 — fields default to zero.
        let mut packet = [0u8; 31];
        packet[0] = 0x20;
        packet[1] = 0x01;
        packet[2] = 0; // length = 0
        let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        packet[3] = packet[3].wrapping_sub(sum);
        let decoded = DynamicHdrFragment::decode(&packet).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value.chunk_len, 0);
    }

    // --- decode_sequence tests ---

    #[test]
    fn decode_sequence_single_packet_unknown_format() {
        let packet = make_packet(0, 23, 0x04, &[0xAAu8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[packet]).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(
            decoded.value,
            DynamicHdrInfoFrame::Unknown { format_id: 0x04 }
        );
    }

    #[test]
    fn decode_sequence_multi_packet_format_id_from_first() {
        let p0 = make_packet(0, 46, 0x04, &[0xAAu8; 23]);
        let p1 = make_packet(1, 46, 0x04, &[0xBBu8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0, p1]).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(
            decoded.value,
            DynamicHdrInfoFrame::Unknown { format_id: 0x04 }
        );
    }

    #[test]
    fn decode_sequence_empty_yields_unknown_zero() {
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[]).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(decoded.value, DynamicHdrInfoFrame::Unknown { format_id: 0 });
    }

    #[test]
    fn decode_sequence_checksum_mismatch_warning() {
        let mut p0 = make_packet(0, 23, 0x04, &[0u8; 23]);
        p0[3] = p0[3].wrapping_add(1); // corrupt checksum
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0]).unwrap();
        assert!(
            decoded
                .iter_warnings()
                .any(|w| matches!(w, DynamicHdrWarning::ChecksumMismatch { .. }))
        );
    }

    #[test]
    fn decode_sequence_truncated_returns_error() {
        let mut p0 = make_packet(0, 23, 0x04, &[0u8; 23]);
        p0[2] = 28; // > 27
        assert!(matches!(
            DynamicHdrInfoFrame::decode_sequence(&[p0]),
            Err(DecodeError::Truncated { claimed: 28 })
        ));
    }
}
