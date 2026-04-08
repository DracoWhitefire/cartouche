use crate::decoded::Decoded;
use crate::encode::IntoPackets;
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;

/// Maximum byte length of an assembled Dynamic HDR metadata payload.
///
/// Sized for a worst-case HDR10+ frame (ETSI TS 103 433-1 §6.1, all optional
/// fields populated including two 25×25 `ActualPeakLuminance` tables):
/// approximately 580 bytes. Used as the stack-buffer size in bare `no_std`
/// builds where heap allocation is unavailable.
pub(crate) const MAX_DYNAMIC_HDR_PAYLOAD: usize = 600;

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
    /// recognised by this version of `cartouche`. In `alloc`/`std` builds the
    /// raw metadata bytes are retained in `payload`, making this variant
    /// re-encodable. In bare `no_std` builds the payload is not retained.
    Unknown {
        /// The metadata format identifier from the first packet in the sequence.
        format_id: u8,
        /// Raw metadata bytes concatenated from all chunks in the sequence.
        ///
        /// Only present in `alloc`/`std` builds. In bare `no_std` builds the
        /// payload is not retained.
        #[cfg(any(feature = "alloc", feature = "std"))]
        payload: alloc::vec::Vec<u8>,
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
        if packets.is_empty() {
            return Err(DecodeError::EmptySequence);
        }

        // Read sequence-level invariants from the first packet.
        let format_id = packets[0][7];
        let total_bytes = u16::from_le_bytes([packets[0][5], packets[0][6]]);

        let mut decoded = Decoded::new(DynamicHdrInfoFrame::Unknown {
            format_id,
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload: alloc::vec::Vec::new(),
        });

        #[cfg(any(feature = "alloc", feature = "std"))]
        let mut payload: alloc::vec::Vec<u8> = alloc::vec::Vec::new();

        for (i, packet) in packets.iter().enumerate() {
            let length = packet[2];
            if length > 27 {
                return Err(DecodeError::Truncated { claimed: length });
            }

            // Checksum.
            let sum: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
            if sum != 0x00 {
                let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
                decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
                    expected,
                    found: packet[3],
                });
            }

            // Sequence integrity.
            let seq_num = packet[4];
            if seq_num != i as u8 {
                decoded.push_warning(DynamicHdrWarning::OutOfOrderPacket {
                    index: i as u8,
                    found: seq_num,
                });
            }
            let pkt_total = u16::from_le_bytes([packet[5], packet[6]]);
            if pkt_total != total_bytes {
                decoded.push_warning(DynamicHdrWarning::InconsistentTotalBytes {
                    packet: i as u8,
                    expected: total_bytes,
                    found: pkt_total,
                });
            }
            let pkt_fmt = packet[7];
            if pkt_fmt != format_id {
                decoded.push_warning(DynamicHdrWarning::InconsistentFormatId {
                    packet: i as u8,
                    expected: format_id,
                    found: pkt_fmt,
                });
            }

            // Chunk accumulation.
            // chunk_len = payload bytes after the 4-byte per-packet overhead, capped at 23.
            #[cfg(any(feature = "alloc", feature = "std"))]
            {
                let chunk_len = length.saturating_sub(4).min(23) as usize;
                payload.extend_from_slice(&packet[8..8 + chunk_len]);
            }
        }

        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            decoded.value = DynamicHdrInfoFrame::Unknown { format_id, payload };
        }

        Ok(decoded)
    }
}

/// Iterator that yields 31-byte wire packets for a [`DynamicHdrInfoFrame`].
///
/// Produced by [`DynamicHdrInfoFrame::into_packets`]. Yields one packet per
/// 23-byte chunk of the serialized metadata payload; the final packet carries
/// any remaining bytes (fewer than 23).
pub struct DynamicHdrIter {
    format_id: u8,
    total_bytes: u16,
    offset: usize,
    seq_num: u8,
    #[cfg(any(feature = "alloc", feature = "std"))]
    payload: alloc::vec::Vec<u8>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    /// Length of valid bytes in `payload` (bare `no_std` builds only).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload_len: usize,
}

impl Iterator for DynamicHdrIter {
    type Item = [u8; 31];

    fn next(&mut self) -> Option<[u8; 31]> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        let payload_len = self.payload.len();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        let payload_len = self.payload_len;

        if self.offset >= payload_len {
            return None;
        }

        let chunk_len = (payload_len - self.offset).min(23);

        // Build the 30 non-checksum bytes: [type, version, length, PB0..PB26].
        // Byte layout (offsets in the final 31-byte packet):
        //   0: type_code=0x20, 1: version=0x01, 2: length=4+chunk_len
        //   3: checksum (filled below), 4: seq_num, 5–6: total_bytes LE,
        //   7: format_id, 8..8+chunk_len: chunk data
        let mut hp = [0u8; 30];
        hp[0] = 0x20; // Dynamic HDR type code
        hp[1] = 0x01; // version
        hp[2] = (4 + chunk_len) as u8;
        hp[3] = self.seq_num;
        let tb = self.total_bytes.to_le_bytes();
        hp[4] = tb[0];
        hp[5] = tb[1];
        hp[6] = self.format_id;
        hp[7..7 + chunk_len].copy_from_slice(&self.payload[self.offset..self.offset + chunk_len]);

        let checksum = crate::checksum::compute_checksum(&hp);

        let mut packet = [0u8; 31];
        packet[..3].copy_from_slice(&hp[..3]);
        packet[3] = checksum;
        packet[4..].copy_from_slice(&hp[3..]);

        self.offset += chunk_len;
        self.seq_num += 1;

        Some(packet)
    }
}

impl IntoPackets for DynamicHdrInfoFrame {
    type Iter = DynamicHdrIter;
    type Warning = DynamicHdrWarning;

    fn into_packets(self) -> Decoded<DynamicHdrIter, DynamicHdrWarning> {
        match self {
            DynamicHdrInfoFrame::Unknown {
                format_id,
                #[cfg(any(feature = "alloc", feature = "std"))]
                payload,
            } => {
                #[cfg(any(feature = "alloc", feature = "std"))]
                let total_bytes = payload.len() as u16;
                #[cfg(not(any(feature = "alloc", feature = "std")))]
                let total_bytes: u16 = 0;

                Decoded::new(DynamicHdrIter {
                    format_id,
                    total_bytes,
                    offset: 0,
                    seq_num: 0,
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    payload,
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload: [0u8; MAX_DYNAMIC_HDR_PAYLOAD],
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload_len: 0,
                })
            }
        }
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
    #[cfg(any(feature = "alloc", feature = "std"))]
    use alloc::vec;

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
    fn decode_sequence_empty_returns_error() {
        assert!(matches!(
            DynamicHdrInfoFrame::decode_sequence(&[]),
            Err(DecodeError::EmptySequence)
        ));
    }

    #[test]
    fn decode_sequence_single_packet_unknown_format() {
        let packet = make_packet(0, 23, 0x04, &[0xAAu8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[packet]).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(
            decoded.value,
            DynamicHdrInfoFrame::Unknown {
                format_id: 0x04,
                #[cfg(any(feature = "alloc", feature = "std"))]
                payload: vec![0xAAu8; 23],
            }
        );
    }

    #[test]
    fn decode_sequence_multi_packet_payload_assembled() {
        let p0 = make_packet(0, 46, 0x04, &[0xAAu8; 23]);
        let p1 = make_packet(1, 46, 0x04, &[0xBBu8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0, p1]).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            let expected: alloc::vec::Vec<u8> = [0xAAu8; 23]
                .iter()
                .chain([0xBBu8; 23].iter())
                .copied()
                .collect();
            assert_eq!(
                decoded.value,
                DynamicHdrInfoFrame::Unknown {
                    format_id: 0x04,
                    payload: expected,
                }
            );
        }
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

    #[test]
    fn decode_sequence_out_of_order_seq_num_warning() {
        // seq_num = 5 in a packet at index 0.
        let mut p0 = make_packet(0, 23, 0x04, &[0u8; 23]);
        p0[4] = 5;
        // Recompute checksum after tampering.
        let sum: u8 = p0.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        p0[3] = p0[3].wrapping_sub(sum);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0]).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            DynamicHdrWarning::OutOfOrderPacket { index: 0, found: 5 }
        )));
    }

    #[test]
    fn decode_sequence_inconsistent_total_bytes_warning() {
        let p0 = make_packet(0, 46, 0x04, &[0u8; 23]);
        // p1 declares a different total_bytes.
        let p1 = make_packet(1, 99, 0x04, &[0u8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0, p1]).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            DynamicHdrWarning::InconsistentTotalBytes {
                packet: 1,
                expected: 46,
                found: 99,
            }
        )));
    }

    #[test]
    fn decode_sequence_inconsistent_format_id_warning() {
        let p0 = make_packet(0, 46, 0x04, &[0u8; 23]);
        let p1 = make_packet(1, 46, 0x02, &[0u8; 23]);
        let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0, p1]).unwrap();
        assert!(decoded.iter_warnings().any(|w| matches!(
            w,
            DynamicHdrWarning::InconsistentFormatId {
                packet: 1,
                expected: 0x04,
                found: 0x02,
            }
        )));
    }

    // --- IntoPackets / round-trip tests ---

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn decode_sequence_unknown_payload_roundtrip() {
        use crate::encode::IntoPackets;

        // 50-byte payload → 3 packets (23 + 23 + 4).
        let original_payload: alloc::vec::Vec<u8> = (0u8..50).collect();
        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x07,
            payload: original_payload.clone(),
        };

        let encoded = frame.into_packets();
        assert!(encoded.iter_warnings().next().is_none());

        let packets: alloc::vec::Vec<[u8; 31]> = encoded.value.collect();
        assert_eq!(packets.len(), 3);

        let decoded = DynamicHdrInfoFrame::decode_sequence(&packets).unwrap();
        assert!(decoded.iter_warnings().next().is_none());
        assert_eq!(
            decoded.value,
            DynamicHdrInfoFrame::Unknown {
                format_id: 0x07,
                payload: original_payload,
            }
        );
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn into_packets_seq_nums_sequential() {
        use crate::encode::IntoPackets;

        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            payload: alloc::vec![0u8; 50],
        };
        for (expected_seq, packet) in frame.into_packets().value.enumerate() {
            assert_eq!(packet[4], expected_seq as u8);
        }
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn into_packets_total_bytes_consistent() {
        use crate::encode::IntoPackets;

        let payload_len: u16 = 50;
        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            payload: alloc::vec![0u8; payload_len as usize],
        };
        for packet in frame.into_packets().value {
            let tb = u16::from_le_bytes([packet[5], packet[6]]);
            assert_eq!(tb, payload_len);
        }
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn into_packets_all_checksums_valid() {
        use crate::encode::IntoPackets;

        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            payload: alloc::vec![0xABu8; 50],
        };
        for packet in frame.into_packets().value {
            let sum: u8 = packet.iter().fold(0u8, |a, &b| a.wrapping_add(b));
            assert_eq!(sum, 0, "packet checksum must make all-bytes sum equal 0");
        }
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn into_packets_final_partial_chunk() {
        use crate::encode::IntoPackets;

        // 24 bytes → 2 packets: first full (23 bytes), second partial (1 byte).
        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            payload: alloc::vec![0xFFu8; 24],
        };
        let packets: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
        assert_eq!(packets.len(), 2);
        // length field = 4 + chunk_len
        assert_eq!(packets[0][2], 4 + 23);
        assert_eq!(packets[1][2], 4 + 1);
        // trailing bytes of the second packet must be zero-padded.
        for &b in &packets[1][9..] {
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn into_packets_empty_payload_yields_no_packets() {
        use crate::encode::IntoPackets;

        let frame = DynamicHdrInfoFrame::Unknown {
            format_id: 0x04,
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload: alloc::vec![],
        };
        assert!(frame.into_packets().value.next().is_none());
    }
}
