use super::*;
#[cfg(any(feature = "alloc", feature = "std"))]
use alloc::vec;

// --- BitReader tests ---

#[test]
fn bit_reader_single_byte_full() {
    let mut r = BitReader::new(&[0b1010_1010]);
    assert_eq!(r.read_u8(8).unwrap(), 0b1010_1010);
    assert_eq!(r.remaining_bits(), 0);
}

#[test]
fn bit_reader_msb_first_ordering() {
    // Read 4 bits then 4 bits from 0b1100_0011.
    let mut r = BitReader::new(&[0b1100_0011]);
    assert_eq!(r.read_u8(4).unwrap(), 0b1100);
    assert_eq!(r.read_u8(4).unwrap(), 0b0011);
}

#[test]
fn bit_reader_spans_byte_boundary() {
    // Read 3 bits from byte 0 and 5 bits that straddle into byte 1.
    // data = [0b101_00000, 0b11111_000]
    // read_u8(8) starting at bit 5 of byte 0 should give bits 5,6,7 of
    // byte 0 and bits 0,1,2,3,4 of byte 1.
    let mut r = BitReader::new(&[0b10100000, 0b11111000]);
    let _ = r.read_u8(5).unwrap(); // consume first 5 bits
    assert_eq!(r.read_u8(8).unwrap(), 0b000_11111);
}

#[test]
fn bit_reader_read_bool() {
    let mut r = BitReader::new(&[0b1000_0000]);
    assert!(r.read_bool().unwrap());
    assert!(!r.read_bool().unwrap());
}

#[test]
fn bit_reader_read_u32_wide() {
    // Pack 0xDEAD_BEEF into 4 bytes and read it back as 32 bits.
    let data = 0xDEAD_BEEFu32.to_be_bytes();
    let mut r = BitReader::new(&data);
    assert_eq!(r.read_u32(32).unwrap(), 0xDEAD_BEEF);
}

#[test]
fn bit_reader_short_read_is_error() {
    let mut r = BitReader::new(&[0xFFu8]);
    let _ = r.read_u8(8).unwrap();
    assert!(matches!(r.read_u8(1), Err(DecodeError::MalformedPayload)));
}

// --- BitWriter tests ---

#[test]
fn bit_writer_single_byte_full() {
    let mut w = BitWriter::new();
    w.write_u8(0b1010_1010, 8);
    let (buf, len) = w.finish();
    assert_eq!(len, 1);
    assert_eq!(buf[0], 0b1010_1010);
}

#[test]
fn bit_writer_msb_first_ordering() {
    let mut w = BitWriter::new();
    w.write_u8(0b1100, 4);
    w.write_u8(0b0011, 4);
    let (buf, len) = w.finish();
    assert_eq!(len, 1);
    assert_eq!(buf[0], 0b1100_0011);
}

#[test]
fn bit_writer_spans_byte_boundary() {
    let mut w = BitWriter::new();
    w.write_u8(0b10111, 5); // top 5 bits of byte 0
    w.write_u8(0b110, 3); // remaining 3 bits of byte 0
    w.write_u8(0b01010101, 8); // byte 1
    let (buf, len) = w.finish();
    assert_eq!(len, 2);
    assert_eq!(buf[0], 0b10111_110);
    assert_eq!(buf[1], 0b01010101);
}

#[test]
fn bit_writer_write_bool() {
    let mut w = BitWriter::new();
    w.write_bool(true);
    w.write_bool(false);
    w.write_bool(true);
    // Remaining 5 bits are zero → byte = 0b101_00000
    let (buf, len) = w.finish();
    assert_eq!(len, 1);
    assert_eq!(buf[0], 0b1010_0000);
}

#[test]
fn bit_writer_round_trip_with_reader() {
    let mut w = BitWriter::new();
    w.write_u32(0xDEAD_BEEF, 32);
    w.write_u8(0b101, 3);
    w.write_u16(0x1234, 13);
    let (buf, len) = w.finish();

    let mut r = BitReader::new(&buf[..len]);
    assert_eq!(r.read_u32(32).unwrap(), 0xDEAD_BEEF);
    assert_eq!(r.read_u8(3).unwrap(), 0b101);
    assert_eq!(r.read_u16(13).unwrap(), 0x1234);
    assert_eq!(r.remaining_bits(), 0);
}

#[test]
fn bit_writer_partial_final_byte() {
    // Writing 9 bits should consume exactly 2 bytes (1 full + 1 partial).
    let mut w = BitWriter::new();
    w.write_u16(0b1_1111_1111, 9);
    let (buf, len) = w.finish();
    assert_eq!(len, 2);
    assert_eq!(buf[0], 0b1111_1111);
    assert_eq!(buf[1] >> 7, 1); // top bit of second byte
}

#[test]
fn bit_reader_remaining_bits() {
    let mut r = BitReader::new(&[0xFF, 0xFF]);
    assert_eq!(r.remaining_bits(), 16);
    let _ = r.read_u8(3).unwrap();
    assert_eq!(r.remaining_bits(), 13);
}

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
    let mut packet = make_packet(0, 23, 0xFF, &[0u8; 23]);
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
    let packet = make_packet(0, 23, 0xFF, &[0xAAu8; 23]);
    let decoded = DynamicHdrInfoFrame::decode_sequence(&[packet]).unwrap();
    assert!(decoded.iter_warnings().next().is_none());
    assert_eq!(
        decoded.value,
        DynamicHdrInfoFrame::Unknown {
            format_id: 0xFF,
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload: vec![0xAAu8; 23],
        }
    );
}

#[test]
fn decode_sequence_multi_packet_payload_assembled() {
    let p0 = make_packet(0, 46, 0xFF, &[0xAAu8; 23]);
    let p1 = make_packet(1, 46, 0xFF, &[0xBBu8; 23]);
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
                format_id: 0xFF,
                payload: expected,
            }
        );
    }
}

#[test]
fn decode_sequence_checksum_mismatch_warning() {
    let mut p0 = make_packet(0, 23, 0xFF, &[0u8; 23]);
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
    let mut p0 = make_packet(0, 23, 0xFF, &[0u8; 23]);
    p0[2] = 28; // > 27
    assert!(matches!(
        DynamicHdrInfoFrame::decode_sequence(&[p0]),
        Err(DecodeError::Truncated { claimed: 28 })
    ));
}

#[test]
fn decode_sequence_out_of_order_seq_num_warning() {
    // seq_num = 5 in a packet at index 0.
    let mut p0 = make_packet(0, 23, 0xFF, &[0u8; 23]);
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
    let p0 = make_packet(0, 46, 0xFF, &[0u8; 23]);
    // p1 declares a different total_bytes.
    let p1 = make_packet(1, 99, 0xFF, &[0u8; 23]);
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
    let p0 = make_packet(0, 46, 0xFF, &[0u8; 23]);
    let p1 = make_packet(1, 46, 0xFE, &[0u8; 23]);
    let decoded = DynamicHdrInfoFrame::decode_sequence(&[p0, p1]).unwrap();
    assert!(decoded.iter_warnings().any(|w| matches!(
        w,
        DynamicHdrWarning::InconsistentFormatId {
            packet: 1,
            expected: 0xFF,
            found: 0xFE,
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
fn decode_sequence_hdr10plus_inner_error_propagates() {
    // Single packet with format_id=0x04 but only 1 byte of payload —
    // too short for Hdr10PlusMetadata::decode → MalformedPayload.
    let pkt = make_packet(0, 1, 0x04, &[0x01]);
    let result = DynamicHdrInfoFrame::decode_sequence(&[pkt]);
    assert!(
        matches!(result, Err(DecodeError::MalformedPayload)),
        "expected MalformedPayload, got {result:?}"
    );
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn decode_sequence_slhdr_inner_error_propagates() {
    // Single packet with format_id=0x02 but only 1 byte of payload —
    // too short for SlHdrMetadata::decode → MalformedPayload.
    let pkt = make_packet(0, 1, 0x02, &[0xB5]);
    let result = DynamicHdrInfoFrame::decode_sequence(&[pkt]);
    assert!(
        matches!(result, Err(DecodeError::MalformedPayload)),
        "expected MalformedPayload, got {result:?}"
    );
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn decode_sequence_format_warnings_forwarded() {
    // A valid HDR10+ payload with reserved bits set causes
    // Hdr10PlusMetadata::decode to emit warnings; decode_sequence must
    // forward them onto the returned Decoded value.
    let (mut payload, _) = make_minimal_hdr10plus_payload();
    payload[2] |= 0b1100_0000; // set both reserved bits
    let total = payload.len() as u16;
    let pkts: alloc::vec::Vec<[u8; 31]> = payload
        .chunks(23)
        .enumerate()
        .map(|(i, chunk)| make_packet(i as u8, total, 0x04, chunk))
        .collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    assert!(
        decoded
            .iter_warnings()
            .any(|w| matches!(w, DynamicHdrWarning::ReservedFieldNonZero { .. })),
        "expected ReservedFieldNonZero warning to be forwarded"
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

// --- Hdr10PlusMetadata::decode tests ---

/// Build an HDR10+ payload exercising all optional fields:
/// application_mode=1 (scene_frame_switching_flag present), both actual-peak-luminance
/// tables populated, distribution_maxrgb with 2 entries, tone_mapping_flag=true with
/// knee point and 2 bezier anchors, color_saturation_mapping_flag=true.
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_full_hdr10plus_payload() -> (alloc::vec::Vec<u8>, Hdr10PlusMetadata) {
    let mut w = BitWriter::new();
    w.write_u8(0x02, 8); // application_identifier
    w.write_u8(0x01, 8); // application_mode = 1 → scene_frame_switching_flag follows
    w.write_bool(true); // scene_frame_switching_flag
    w.write_u8(0, 2); // 2 reserved bits
    w.write_u32(2000, 27); // targeted_system_display_maximum_luminance
    // targeted_system_display_actual_peak_luminance: 2×2 table
    w.write_bool(true);
    w.write_u8(2, 5); // num_rows
    w.write_u8(2, 5); // num_cols
    w.write_u8(3, 4); // [0][0]
    w.write_u8(5, 4); // [0][1]
    w.write_u8(7, 4); // [1][0]
    w.write_u8(9, 4); // [1][1]
    // 1 window (stored as count − 1 = 0)
    w.write_u8(0, 2);
    // Window 0
    w.write_u16(10, 16); // upper_left_corner_x
    w.write_u16(20, 16); // upper_left_corner_y
    w.write_u16(200, 16); // lower_right_corner_x
    w.write_u16(300, 16); // lower_right_corner_y
    w.write_u16(100, 16); // center_of_ellipse_x
    w.write_u16(150, 16); // center_of_ellipse_y
    w.write_u8(45, 8); // rotation_angle
    w.write_u16(50, 16); // semimajor_axis_internal_ellipse
    w.write_u16(60, 16); // semimajor_axis_external_ellipse
    w.write_u16(30, 16); // semiminor_axis_external_ellipse
    w.write_bool(true); // overlap_process_option
    w.write_u32(1000, 17); // maxscl[0]
    w.write_u32(2000, 17); // maxscl[1]
    w.write_u32(3000, 17); // maxscl[2]
    w.write_u32(500, 17); // average_maxrgb
    // distribution_maxrgb: 2 entries
    w.write_u8(2, 4);
    w.write_u8(25, 7); // percentages[0]
    w.write_u32(100, 17); // percentiles[0]
    w.write_u8(75, 7); // percentages[1]
    w.write_u32(800, 17); // percentiles[1]
    w.write_u16(512, 10); // fraction_bright_pixels
    // mastering_display_actual_peak_luminance: 1×1 table
    w.write_bool(true);
    w.write_u8(1, 5); // num_rows
    w.write_u8(1, 5); // num_cols
    w.write_u8(12, 4); // [0][0]
    // Tone-mapping pass (1 window, tone_mapping_flag=true)
    w.write_bool(true);
    w.write_u16(100, 12); // knee_point.x
    w.write_u16(200, 12); // knee_point.y
    w.write_u8(2, 4); // num_anchors
    w.write_u16(300, 10); // anchors[0]
    w.write_u16(400, 10); // anchors[1]
    // color_saturation_mapping_flag=true + weight
    w.write_bool(true);
    w.write_u8(42, 6); // color_saturation_weight
    let (buf, len) = w.finish();
    let payload = buf[..len].to_vec();

    let mut tgt_entries = [[0u8; 25]; 25];
    tgt_entries[0][0] = 3;
    tgt_entries[0][1] = 5;
    tgt_entries[1][0] = 7;
    tgt_entries[1][1] = 9;
    let mut mast_entries = [[0u8; 25]; 25];
    mast_entries[0][0] = 12;
    let mut dist = DistributionMaxrgb {
        count: 2,
        ..Default::default()
    };
    dist.percentages[0] = 25;
    dist.percentiles[0] = 100;
    dist.percentages[1] = 75;
    dist.percentiles[1] = 800;
    let mut bezier = BezierAnchors {
        count: 2,
        ..Default::default()
    };
    bezier.anchors[0] = 300;
    bezier.anchors[1] = 400;

    let expected = Hdr10PlusMetadata {
        application_identifier: 0x02,
        application_mode: 0x01,
        scene_frame_switching_flag: true,
        targeted_system_display_maximum_luminance: 2000,
        targeted_system_display_actual_peak_luminance_flag: true,
        targeted_system_display_actual_peak_luminance: Some(ActualPeakLuminance {
            num_rows: 2,
            num_cols: 2,
            entries: tgt_entries,
        }),
        windows: Hdr10PlusWindows {
            count: 1,
            windows: [
                Hdr10PlusWindow {
                    upper_left_corner_x: 10,
                    upper_left_corner_y: 20,
                    lower_right_corner_x: 200,
                    lower_right_corner_y: 300,
                    center_of_ellipse_x: 100,
                    center_of_ellipse_y: 150,
                    rotation_angle: 45,
                    semimajor_axis_internal_ellipse: 50,
                    semimajor_axis_external_ellipse: 60,
                    semiminor_axis_external_ellipse: 30,
                    overlap_process_option: true,
                    maxscl: [1000, 2000, 3000],
                    average_maxrgb: 500,
                    distribution_maxrgb: dist,
                    fraction_bright_pixels: 512,
                    tone_mapping_flag: true,
                    knee_point: Some(KneePoint { x: 100, y: 200 }),
                    bezier_curve_anchors: bezier,
                },
                Hdr10PlusWindow::default(),
                Hdr10PlusWindow::default(),
            ],
        },
        mastering_display_actual_peak_luminance_flag: true,
        mastering_display_actual_peak_luminance: Some(ActualPeakLuminance {
            num_rows: 1,
            num_cols: 1,
            entries: mast_entries,
        }),
        color_saturation_mapping_flag: true,
        color_saturation_weight: Some(42),
    };
    (payload, expected)
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_full_fields_decode() {
    let (payload, expected) = make_full_hdr10plus_payload();
    let mut warnings = alloc::vec::Vec::new();
    let got = Hdr10PlusMetadata::decode(&payload, &mut |w| warnings.push(w)).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(got, expected);
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_full_fields_round_trip() {
    use crate::encode::IntoPackets;
    let (_, original) = make_full_hdr10plus_payload();
    let frame = DynamicHdrInfoFrame::Hdr10Plus(alloc::boxed::Box::new(original.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::Hdr10Plus(meta) => assert_eq!(*meta, original),
        other => panic!("expected Hdr10Plus, got {other:?}"),
    }
}

/// Build a minimal valid HDR10+ payload using BitWriter:
/// application_mode=0, no optional fields, 1 window with all-zero fields.
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_minimal_hdr10plus_payload() -> (alloc::vec::Vec<u8>, Hdr10PlusMetadata) {
    let mut w = BitWriter::new();
    w.write_u8(0x01, 8); // application_identifier
    w.write_u8(0x00, 8); // application_mode = 0 (scene-based)
    // no scene_frame_switching_flag (mode != 1)
    w.write_u8(0, 2); // 2 reserved bits
    w.write_u32(1000, 27); // targeted_system_display_maximum_luminance
    w.write_bool(false); // targeted_system_display_actual_peak_luminance_flag
    w.write_u8(0, 2); // num_windows − 1 = 0  →  1 window
    // Window 0: all-zero fields, in decode_window() read order.
    for _ in 0..6 {
        w.write_u16(0, 16);
    } // upper/lower corners + center (6 × 16)
    w.write_u8(0, 8); // rotation_angle
    for _ in 0..3 {
        w.write_u16(0, 16);
    } // ellipse semi-axes (3 × 16)
    w.write_bool(false); // overlap_process_option
    for _ in 0..3 {
        w.write_u32(0, 17);
    } // maxscl (3 × 17)
    w.write_u32(0, 17); // average_maxrgb
    w.write_u8(0, 4); // num_distribution_maxrgb_percentiles = 0
    w.write_u16(0, 10); // fraction_bright_pixels
    w.write_bool(false); // mastering_display_actual_peak_luminance_flag
    // Tone mapping pass (1 window)
    w.write_bool(false); // tone_mapping_flag = 0
    w.write_bool(false); // color_saturation_mapping_flag

    let (buf, len) = w.finish();
    let payload = buf[..len].to_vec();

    let expected = Hdr10PlusMetadata {
        application_identifier: 0x01,
        application_mode: 0x00,
        scene_frame_switching_flag: false,
        targeted_system_display_maximum_luminance: 1000,
        targeted_system_display_actual_peak_luminance_flag: false,
        targeted_system_display_actual_peak_luminance: None,
        windows: Hdr10PlusWindows {
            count: 1,
            windows: [
                Hdr10PlusWindow::default(),
                Hdr10PlusWindow::default(),
                Hdr10PlusWindow::default(),
            ],
        },
        mastering_display_actual_peak_luminance_flag: false,
        mastering_display_actual_peak_luminance: None,
        color_saturation_mapping_flag: false,
        color_saturation_weight: None,
    };
    (payload, expected)
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_single_window_no_optional_fields() {
    let (payload, expected) = make_minimal_hdr10plus_payload();
    let mut warnings = alloc::vec::Vec::new();
    let got = Hdr10PlusMetadata::decode(&payload, &mut |w| warnings.push(w)).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(got, expected);
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_malformed_short_payload_is_error() {
    // Empty payload must fail.
    assert!(matches!(
        Hdr10PlusMetadata::decode(&[], &mut |_| {}),
        Err(DecodeError::MalformedPayload)
    ));
    // Truncated mid-stream must also fail.
    assert!(matches!(
        Hdr10PlusMetadata::decode(&[0x01, 0x00], &mut |_| {}),
        Err(DecodeError::MalformedPayload)
    ));
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_too_many_bezier_anchors_is_malformed() {
    // Build a payload with tone_mapping_flag=true and num_anchors=10,
    // which exceeds the 9-slot BezierAnchors array. Must return
    // MalformedPayload, not panic with an index-out-of-bounds.
    let mut w = BitWriter::new();
    w.write_u8(0x01, 8); // application_identifier
    w.write_u8(0x00, 8); // application_mode
    w.write_u8(0, 2); // reserved bits
    w.write_u32(1000, 27); // targeted_system_display_maximum_luminance
    w.write_bool(false); // targeted_system_display_actual_peak_luminance_flag
    w.write_u8(0, 2); // num_windows_minus1 = 0 → 1 window
    // Window 0: minimal fields
    for _ in 0..6 {
        w.write_u16(0, 16);
    } // corners + center
    w.write_u8(0, 8); // rotation_angle
    for _ in 0..3 {
        w.write_u16(0, 16);
    } // semi-axes
    w.write_bool(false); // overlap_process_option
    for _ in 0..3 {
        w.write_u32(0, 17);
    } // maxscl
    w.write_u32(0, 17); // average_maxrgb
    w.write_u8(0, 4); // num_distribution_maxrgb_percentiles = 0
    w.write_u16(0, 10); // fraction_bright_pixels
    w.write_bool(false); // mastering_display_actual_peak_luminance_flag
    // Tone-mapping pass: tone_mapping_flag=true, num_anchors=10 (out of range)
    w.write_bool(true); // tone_mapping_flag
    w.write_u16(0, 12); // knee_point.x
    w.write_u16(0, 12); // knee_point.y
    w.write_u8(10, 4); // num_anchors = 10 (exceeds array capacity of 9)
    let (buf, len) = w.finish();
    assert!(matches!(
        Hdr10PlusMetadata::decode(&buf[..len], &mut |_| {}),
        Err(DecodeError::MalformedPayload)
    ));
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_four_windows_is_malformed() {
    // Build a payload with num_windows_minus1 = 3 (raw), which would imply
    // 4 windows — beyond the 3-slot array capacity. Must return MalformedPayload,
    // not panic with an index-out-of-bounds.
    let mut w = BitWriter::new();
    w.write_u8(0x01, 8); // application_identifier
    w.write_u8(0x00, 8); // application_mode
    w.write_u8(0, 2); // reserved bits
    w.write_u32(1000, 27); // targeted_system_display_maximum_luminance
    w.write_bool(false); // targeted_system_display_actual_peak_luminance_flag
    w.write_u8(3, 2); // num_windows_minus1 = 3 → 4 windows (out of range)
    let (buf, len) = w.finish();
    assert!(matches!(
        Hdr10PlusMetadata::decode(&buf[..len], &mut |_| {}),
        Err(DecodeError::MalformedPayload)
    ));
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_unknown_application_mode_warning() {
    // application_mode=2 is unknown (spec defines 0 and 1 only).
    // Decode must succeed and emit UnknownEnumValue; no scene_frame_switching_flag
    // bit is consumed (same as mode 0).
    let (mut payload, _) = make_minimal_hdr10plus_payload();
    payload[1] = 0x02; // overwrite application_mode byte
    let mut warnings = alloc::vec::Vec::new();
    Hdr10PlusMetadata::decode(&payload, &mut |w| warnings.push(w)).unwrap();
    assert!(warnings.iter().any(|w| matches!(
        w,
        DynamicHdrWarning::UnknownEnumValue {
            field: "application_mode",
            raw: 2
        }
    )));
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_reserved_bits_set_warning() {
    let (mut payload, _) = make_minimal_hdr10plus_payload();
    // The two reserved bits follow byte 1 (application_mode) at bits 6 and 5
    // of byte 2 (MSB-first). Set both by OR-ing 0b1100_0000 into byte 2.
    payload[2] |= 0b1100_0000;
    let mut warnings = alloc::vec::Vec::new();
    Hdr10PlusMetadata::decode(&payload, &mut |w| warnings.push(w)).unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, DynamicHdrWarning::ReservedFieldNonZero { .. }))
    );
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_round_trip() {
    use crate::encode::IntoPackets;

    let (_, original) = make_minimal_hdr10plus_payload();
    let frame = DynamicHdrInfoFrame::Hdr10Plus(alloc::boxed::Box::new(original.clone()));

    // Encode to packets then decode back.
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();

    match decoded.value {
        DynamicHdrInfoFrame::Hdr10Plus(meta) => assert_eq!(*meta, original),
        other => panic!("expected Hdr10Plus, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_decode_sequence_dispatches_to_hdr10plus_variant() {
    let (payload, _) = make_minimal_hdr10plus_payload();
    let total = payload.len() as u16;
    // Split into 23-byte chunks across multiple packets.
    let pkts: alloc::vec::Vec<[u8; 31]> = payload
        .chunks(23)
        .enumerate()
        .map(|(i, chunk)| make_packet(i as u8, total, 0x04, chunk))
        .collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    assert!(matches!(decoded.value, DynamicHdrInfoFrame::Hdr10Plus(_)));
}

// --- SlHdrMetadata::decode tests ---

/// Build an SL-HDR mode-0 payload with all optional body fields present:
/// original_picture_info, target_picture_info, src_mdcv_info, and mode-0
/// fine-tuning (2 entries) and saturation-gain (1 entry) tables.
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_slhdr_full_body_payload() -> (alloc::vec::Vec<u8>, SlHdrMetadata) {
    let mut w = BitWriter::new();
    // Header
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4); // sl_hdr_mode_value_minus1
    w.write_u8(1, 4); // sl_hdr_spec_major_version_idc
    w.write_u8(0, 7); // sl_hdr_spec_minor_version_idc
    w.write_bool(false); // sl_hdr_cancel_flag
    // Body flags — all optional blocks present, mode 0
    w.write_bool(true); // sl_hdr_persistence_flag
    w.write_bool(true); // original_picture_info_present_flag
    w.write_bool(true); // target_picture_info_present_flag
    w.write_bool(true); // src_mdcv_info_present_flag
    w.write_bool(false); // sl_hdr_extension_present_flag
    w.write_u8(0, 3); // sl_hdr_payload_mode = 0
    // original_picture_info
    w.write_u8(1, 8);
    w.write_u16(1000, 16);
    w.write_u16(5, 16);
    // target_picture_info
    w.write_u8(9, 8);
    w.write_u16(400, 16);
    w.write_u16(1, 16);
    // src_mdcv_info: primaries (3 × x,y), ref_white, mastering luminance
    w.write_u16(100, 16);
    w.write_u16(200, 16); // primaries[0]
    w.write_u16(300, 16);
    w.write_u16(400, 16); // primaries[1]
    w.write_u16(500, 16);
    w.write_u16(600, 16); // primaries[2]
    w.write_u16(700, 16);
    w.write_u16(800, 16); // ref_white_x, ref_white_y
    w.write_u16(900, 16);
    w.write_u16(10, 16); // max/min mastering luminance
    // matrix_coefficient_values (4 × 16)
    for v in [1u16, 2, 3, 4] {
        w.write_u16(v, 16);
    }
    // chroma_to_luma_injection (2 × 16)
    for v in [5u16, 6] {
        w.write_u16(v, 16);
    }
    // k_coefficient_values (3 × 8)
    for v in [7u8, 8, 9] {
        w.write_u8(v, 8);
    }
    // Mode 0: five scalars, then ftm_count(4), sg_count(4), ftm entries, sg entries
    w.write_u8(10, 8); // black_level_offset
    w.write_u8(20, 8); // white_level_offset
    w.write_u8(30, 8); // shadow_gain_control
    w.write_u8(40, 8); // highlight_gain_control
    w.write_u8(50, 8); // mid_tone_width_adjustment_factor
    w.write_u8(2, 4); // tone_mapping_output_fine_tuning count = 2
    w.write_u8(1, 4); // saturation_gain count = 1
    w.write_u8(11, 8);
    w.write_u8(22, 8); // ftm[0]: x, y
    w.write_u8(33, 8);
    w.write_u8(44, 8); // ftm[1]: x, y
    w.write_u8(55, 8);
    w.write_u8(66, 8); // sg[0]: x, y
    let (buf, len) = w.finish();
    let payload = buf[..len].to_vec();

    let mut ftm = SlHdrTable15 {
        count: 2,
        ..Default::default()
    };
    ftm.x[0] = 11;
    ftm.y[0] = 22;
    ftm.x[1] = 33;
    ftm.y[1] = 44;
    let mut sg = SlHdrTable15 {
        count: 1,
        ..Default::default()
    };
    sg.x[0] = 55;
    sg.y[0] = 66;

    let expected = SlHdrMetadata {
        itu_t_t35_country_code: 0xB5,
        terminal_provider_code: 0x003C,
        terminal_provider_oriented_code_message_idc: 0x01,
        sl_hdr_mode_value_minus1: 0,
        sl_hdr_spec_major_version_idc: 1,
        sl_hdr_spec_minor_version_idc: 0,
        sl_hdr_cancel_flag: false,
        body: Some(SlHdrBody {
            sl_hdr_persistence_flag: true,
            sl_hdr_payload_mode: 0,
            original_picture_info: Some(SlHdrPictureInfo {
                primaries: 1,
                max_luminance: 1000,
                min_luminance: 5,
            }),
            target_picture_info: Some(SlHdrPictureInfo {
                primaries: 9,
                max_luminance: 400,
                min_luminance: 1,
            }),
            src_mdcv_info: Some(SlHdrMdcvInfo {
                primaries: [[100, 200], [300, 400], [500, 600]],
                ref_white_x: 700,
                ref_white_y: 800,
                max_mastering_luminance: 900,
                min_mastering_luminance: 10,
            }),
            matrix_coefficient_values: [1, 2, 3, 4],
            chroma_to_luma_injection: [5, 6],
            k_coefficient_values: [7, 8, 9],
            payload: SlHdrPayload::Mode0(SlHdrMode0 {
                tone_mapping_input_signal_black_level_offset: 10,
                tone_mapping_input_signal_white_level_offset: 20,
                shadow_gain_control: 30,
                highlight_gain_control: 40,
                mid_tone_width_adjustment_factor: 50,
                tone_mapping_output_fine_tuning: ftm,
                saturation_gain: sg,
            }),
            extension: None,
        }),
    };
    (payload, expected)
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_full_body_decode() {
    let (payload, expected) = make_slhdr_full_body_payload();
    let got = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    assert_eq!(got, expected);
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_full_body_round_trip() {
    use crate::encode::IntoPackets;
    let (_, original) = make_slhdr_full_body_payload();
    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

/// Build a minimal SL-HDR payload with `sl_hdr_cancel_flag = true`.
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_slhdr_cancelled_payload() -> alloc::vec::Vec<u8> {
    let mut w = BitWriter::new();
    w.write_u8(0xB5, 8); // itu_t_t35_country_code
    w.write_u16(0x003C, 16); // terminal_provider_code
    w.write_u8(0x01, 8); // terminal_provider_oriented_code_message_idc
    w.write_u8(0, 4); // sl_hdr_mode_value_minus1
    w.write_u8(1, 4); // sl_hdr_spec_major_version_idc
    w.write_u8(0, 7); // sl_hdr_spec_minor_version_idc
    w.write_bool(true); // sl_hdr_cancel_flag = true → no body
    let (buf, len) = w.finish();
    buf[..len].to_vec()
}

/// Build a minimal SL-HDR mode-0 payload (no optional info blocks,
/// no extension, empty fine-tuning and saturation-gain tables).
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_slhdr_mode0_payload() -> alloc::vec::Vec<u8> {
    let mut w = BitWriter::new();
    // Header
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4); // sl_hdr_mode_value_minus1
    w.write_u8(1, 4); // sl_hdr_spec_major_version_idc
    w.write_u8(0, 7); // sl_hdr_spec_minor_version_idc
    w.write_bool(false); // sl_hdr_cancel_flag = false
    // Body flags
    w.write_bool(true); // sl_hdr_persistence_flag
    w.write_bool(false); // original_picture_info_present_flag
    w.write_bool(false); // target_picture_info_present_flag
    w.write_bool(false); // src_mdcv_info_present_flag
    w.write_bool(false); // sl_hdr_extension_present_flag
    w.write_u8(0, 3); // sl_hdr_payload_mode = 0
    // matrix_coefficient_values (4 × 16)
    for _ in 0..4 {
        w.write_u16(0, 16);
    }
    // chroma_to_luma_injection (2 × 16)
    for _ in 0..2 {
        w.write_u16(0, 16);
    }
    // k_coefficient_values (3 × 8)
    for _ in 0..3 {
        w.write_u8(0, 8);
    }
    // Mode 0 fields
    w.write_u8(10, 8); // black_level_offset
    w.write_u8(20, 8); // white_level_offset
    w.write_u8(30, 8); // shadow_gain_control
    w.write_u8(40, 8); // highlight_gain_control
    w.write_u8(50, 8); // mid_tone_width_adjustment_factor
    w.write_u8(0, 4); // tone_mapping_output_fine_tuning_num_val = 0
    w.write_u8(0, 4); // saturation_gain_num_val = 0
    let (buf, len) = w.finish();
    buf[..len].to_vec()
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_cancel_flag_produces_no_body() {
    let payload = make_slhdr_cancelled_payload();
    let meta = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    assert!(meta.sl_hdr_cancel_flag);
    assert!(meta.body.is_none());
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_cancelled_round_trip() {
    use crate::encode::IntoPackets;

    let payload = make_slhdr_cancelled_payload();
    let original = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_unknown_payload_mode_warning() {
    // Build a minimal SL-HDR body with sl_hdr_payload_mode = 7 (unknown).
    let mut w = BitWriter::new();
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4);
    w.write_u8(1, 4);
    w.write_u8(0, 7);
    w.write_bool(false); // sl_hdr_cancel_flag
    w.write_bool(false); // sl_hdr_persistence_flag
    w.write_bool(false); // original_picture_info_present_flag
    w.write_bool(false); // target_picture_info_present_flag
    w.write_bool(false); // src_mdcv_info_present_flag
    w.write_bool(false); // sl_hdr_extension_present_flag
    w.write_u8(7, 3); // sl_hdr_payload_mode = 7 (unknown)
    // matrix, chroma, k all zero
    for _ in 0..4 {
        w.write_u16(0, 16);
    }
    for _ in 0..2 {
        w.write_u16(0, 16);
    }
    for _ in 0..3 {
        w.write_u8(0, 8);
    }
    // No mode-specific bits for unknown mode.
    let (buf, len) = w.finish();
    let payload = &buf[..len];

    let mut warnings = alloc::vec::Vec::new();
    let meta = SlHdrMetadata::decode(payload, &mut |w| warnings.push(w)).unwrap();
    assert!(warnings.iter().any(|w| matches!(
        w,
        DynamicHdrWarning::UnknownEnumValue {
            field: "sl_hdr_payload_mode",
            raw: 7
        }
    )));
    let body = meta.body.as_ref().unwrap();
    assert!(matches!(body.payload, SlHdrPayload::Unknown(7)));

    // Encoding must not panic and must round-trip the struct.
    use crate::encode::IntoPackets;
    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(meta.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(re_meta) => assert_eq!(re_meta.body, meta.body),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_extension_round_trip() {
    use crate::encode::IntoPackets;

    // Build mode-0 payload with extension_present=true, 2 extension bytes.
    let mut w = BitWriter::new();
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4);
    w.write_u8(1, 4);
    w.write_u8(0, 7);
    w.write_bool(false); // sl_hdr_cancel_flag
    w.write_bool(false); // sl_hdr_persistence_flag
    w.write_bool(false); // original_picture_info_present_flag
    w.write_bool(false); // target_picture_info_present_flag
    w.write_bool(false); // src_mdcv_info_present_flag
    w.write_bool(true); // sl_hdr_extension_present_flag
    w.write_u8(0, 3); // sl_hdr_payload_mode = 0
    for _ in 0..4 {
        w.write_u16(0, 16);
    } // matrix
    for _ in 0..2 {
        w.write_u16(0, 16);
    } // chroma
    for _ in 0..3 {
        w.write_u8(0, 8);
    } // k
    // Mode 0: all zero, empty tables
    w.write_u8(0, 8);
    w.write_u8(0, 8);
    w.write_u8(0, 8);
    w.write_u8(0, 8);
    w.write_u8(0, 8);
    w.write_u8(0, 4);
    w.write_u8(0, 4); // ftm_count=0, sg_count=0
    // Extension block
    w.write_u8(0x3F, 6); // extension_6bits
    w.write_u16(2, 10); // length = 2
    w.write_u8(0xAB, 8); // data[0]
    w.write_u8(0xCD, 8); // data[1]
    let (buf, len) = w.finish();
    let payload = buf[..len].to_vec();

    let original = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    let ext = original.body.as_ref().unwrap().extension.as_ref().unwrap();
    assert_eq!(ext.extension_6bits, 0x3F);
    assert_eq!(ext.data, alloc::vec![0xAB, 0xCD]);

    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_mode0_decode() {
    let payload = make_slhdr_mode0_payload();
    let meta = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    assert!(!meta.sl_hdr_cancel_flag);
    let body = meta.body.as_ref().unwrap();
    assert_eq!(body.sl_hdr_payload_mode, 0);
    match &body.payload {
        SlHdrPayload::Mode0(m) => {
            assert_eq!(m.tone_mapping_input_signal_black_level_offset, 10);
            assert_eq!(m.tone_mapping_input_signal_white_level_offset, 20);
            assert_eq!(m.shadow_gain_control, 30);
            assert_eq!(m.highlight_gain_control, 40);
            assert_eq!(m.mid_tone_width_adjustment_factor, 50);
            assert_eq!(m.tone_mapping_output_fine_tuning.count, 0);
            assert_eq!(m.saturation_gain.count, 0);
        }
        other => panic!("expected Mode0, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_malformed_short_payload_is_error() {
    assert!(matches!(
        SlHdrMetadata::decode(&[], &mut |_| {}),
        Err(DecodeError::MalformedPayload)
    ));
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_decode_sequence_dispatches_to_slhdr_variant() {
    let payload = make_slhdr_cancelled_payload();
    let total = payload.len() as u16;
    let pkts: alloc::vec::Vec<[u8; 31]> = payload
        .chunks(23)
        .enumerate()
        .map(|(i, chunk)| make_packet(i as u8, total, 0x02, chunk))
        .collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    assert!(matches!(decoded.value, DynamicHdrInfoFrame::SlHdr(_)));
}

/// Build an SL-HDR mode-1 payload with the opposite sampling flags to
/// `make_slhdr_mode1_payload`: luminance-mapping is *uniform* (no x values)
/// and colour-correction is *non-uniform* (x values present).
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_slhdr_mode1_alt_sampling_payload() -> alloc::vec::Vec<u8> {
    let mut w = BitWriter::new();
    // Header
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4);
    w.write_u8(1, 4);
    w.write_u8(0, 7);
    w.write_bool(false); // sl_hdr_cancel_flag
    w.write_bool(true); // sl_hdr_persistence_flag
    w.write_bool(false);
    w.write_bool(false);
    w.write_bool(false); // optional info absent
    w.write_bool(false); // no extension
    w.write_u8(1, 3); // sl_hdr_payload_mode = 1
    for _ in 0..4 {
        w.write_u16(0, 16);
    } // matrix
    for _ in 0..2 {
        w.write_u16(0, 16);
    } // chroma
    for _ in 0..3 {
        w.write_u8(0, 8);
    } // k
    // Mode 1: luminance_mapping uniform (no x), 2 entries
    w.write_bool(true); // lm_uniform_sampling_flag = true
    w.write_u8(2, 7); // lm_count = 2
    w.write_u16(100, 16); // y[0]
    w.write_u16(200, 16); // y[1]
    // colour_correction non-uniform (x present), 1 entry
    w.write_bool(false); // cc_uniform_sampling_flag = false
    w.write_u8(1, 7); // cc_count = 1
    w.write_u16(300, 16); // x[0]
    w.write_u16(400, 16); // y[0]
    let (buf, len) = w.finish();
    buf[..len].to_vec()
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_mode1_alt_sampling_round_trip() {
    use crate::encode::IntoPackets;

    let payload = make_slhdr_mode1_alt_sampling_payload();
    let original = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();

    let body = original.body.as_ref().unwrap();
    match &body.payload {
        SlHdrPayload::Mode1(m) => {
            assert!(m.lm_uniform_sampling_flag);
            assert_eq!(m.luminance_mapping.count, 2);
            assert_eq!(m.luminance_mapping.y[0], 100);
            assert_eq!(m.luminance_mapping.y[1], 200);
            assert!(!m.cc_uniform_sampling_flag);
            assert_eq!(m.colour_correction.count, 1);
            assert_eq!(m.colour_correction.x[0], 300);
            assert_eq!(m.colour_correction.y[0], 400);
        }
        other => panic!("expected Mode1, got {other:?}"),
    }

    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));
    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();
    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

/// Build a minimal SL-HDR mode-1 payload: 2 luminance-mapping entries
/// (non-uniform) and 1 colour-correction entry (uniform, so no x values).
#[cfg(any(feature = "alloc", feature = "std"))]
fn make_slhdr_mode1_payload() -> alloc::vec::Vec<u8> {
    let mut w = BitWriter::new();
    // Header
    w.write_u8(0xB5, 8);
    w.write_u16(0x003C, 16);
    w.write_u8(0x01, 8);
    w.write_u8(0, 4); // sl_hdr_mode_value_minus1
    w.write_u8(1, 4); // sl_hdr_spec_major_version_idc
    w.write_u8(0, 7); // sl_hdr_spec_minor_version_idc
    w.write_bool(false); // sl_hdr_cancel_flag = false
    // Body flags
    w.write_bool(true); // sl_hdr_persistence_flag
    w.write_bool(false); // original_picture_info_present_flag
    w.write_bool(false); // target_picture_info_present_flag
    w.write_bool(false); // src_mdcv_info_present_flag
    w.write_bool(false); // sl_hdr_extension_present_flag
    w.write_u8(1, 3); // sl_hdr_payload_mode = 1
    // matrix_coefficient_values (4 × 16)
    for _ in 0..4 {
        w.write_u16(0, 16);
    }
    // chroma_to_luma_injection (2 × 16)
    for _ in 0..2 {
        w.write_u16(0, 16);
    }
    // k_coefficient_values (3 × 8)
    for _ in 0..3 {
        w.write_u8(0, 8);
    }
    // Mode 1: luminance_mapping — non-uniform, 2 entries
    w.write_bool(false); // lm_uniform_sampling_flag = false
    w.write_u8(2, 7); // lm_count = 2
    w.write_u16(100, 16); // x[0]
    w.write_u16(200, 16); // y[0]
    w.write_u16(300, 16); // x[1]
    w.write_u16(400, 16); // y[1]
    // Mode 1: colour_correction — uniform, 1 entry
    w.write_bool(true); // cc_uniform_sampling_flag = true
    w.write_u8(1, 7); // cc_count = 1
    w.write_u16(500, 16); // y[0] only (no x for uniform)
    let (buf, len) = w.finish();
    buf[..len].to_vec()
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_mode1_round_trip() {
    use crate::encode::IntoPackets;

    let payload = make_slhdr_mode1_payload();
    let original = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));

    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();

    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_mode0_round_trip() {
    use crate::encode::IntoPackets;

    let payload = make_slhdr_mode0_payload();
    let original = SlHdrMetadata::decode(&payload, &mut |_| {}).unwrap();
    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(original.clone()));

    let pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
    let decoded = DynamicHdrInfoFrame::decode_sequence(&pkts).unwrap();

    match decoded.value {
        DynamicHdrInfoFrame::SlHdr(meta) => assert_eq!(*meta, original),
        other => panic!("expected SlHdr, got {other:?}"),
    }
}

// --- BitWriter overflow regression tests ---

/// Regression for fuzz crash: encode a maximally-populated HDR10+ frame and
/// verify that `into_packets` does not panic (requires MAX_DYNAMIC_HDR_PAYLOAD
/// to be at least the 903-byte worst-case encoded size).
#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn hdr10plus_max_size_encode_no_panic() {
    use crate::encode::IntoPackets;

    // Build an ActualPeakLuminance with 25×25 entries (maximum possible).
    let make_apl = || ActualPeakLuminance {
        num_rows: 25,
        num_cols: 25,
        entries: [[0x0F; 25]; 25],
    };

    // Build a window with 15 distribution_maxrgb entries and 9 bezier anchors.
    let make_window = || Hdr10PlusWindow {
        distribution_maxrgb: DistributionMaxrgb {
            count: 15,
            percentages: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            percentiles: [0xFFFF; 15],
        },
        tone_mapping_flag: true,
        knee_point: Some(KneePoint { x: 0xFFF, y: 0xFFF }),
        bezier_curve_anchors: BezierAnchors {
            count: 9,
            anchors: [0x3FF; 9],
        },
        maxscl: [0x1FFFF; 3],
        average_maxrgb: 0x1FFFF,
        fraction_bright_pixels: 0x3FF,
        ..Default::default()
    };

    let meta = Hdr10PlusMetadata {
        application_identifier: 4,
        application_mode: 1,
        scene_frame_switching_flag: true,
        targeted_system_display_maximum_luminance: 0x07FF_FFFF,
        targeted_system_display_actual_peak_luminance_flag: true,
        targeted_system_display_actual_peak_luminance: Some(make_apl()),
        windows: Hdr10PlusWindows {
            count: 3,
            windows: [make_window(), make_window(), make_window()],
        },
        mastering_display_actual_peak_luminance_flag: true,
        mastering_display_actual_peak_luminance: Some(make_apl()),
        color_saturation_mapping_flag: true,
        color_saturation_weight: Some(0x3F),
    };

    let frame = DynamicHdrInfoFrame::Hdr10Plus(alloc::boxed::Box::new(meta));
    // Must not panic.
    let _pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
}

/// Regression for fuzz crash: encode a maximally-populated SL-HDR mode-1 frame
/// with a maximum-length extension and verify that `into_packets` does not panic
/// (requires MAX_DYNAMIC_HDR_PAYLOAD to be at least the 2107-byte worst-case).
#[test]
#[cfg(any(feature = "alloc", feature = "std"))]
fn slhdr_max_size_encode_no_panic() {
    use crate::encode::IntoPackets;

    // 127-entry mode-1 tables, both non-uniform (x + y per entry).
    let make_table = || SlHdrTable127 {
        count: 127,
        x: [0xFFFF; 127],
        y: [0xFFFF; 127],
    };

    let body = SlHdrBody {
        sl_hdr_persistence_flag: true,
        original_picture_info: Some(SlHdrPictureInfo {
            primaries: 0xFF,
            max_luminance: 0xFFFF,
            min_luminance: 0xFFFF,
        }),
        target_picture_info: Some(SlHdrPictureInfo {
            primaries: 0xFF,
            max_luminance: 0xFFFF,
            min_luminance: 0xFFFF,
        }),
        src_mdcv_info: Some(SlHdrMdcvInfo {
            primaries: [[0xFFFF; 2]; 3],
            ref_white_x: 0xFFFF,
            ref_white_y: 0xFFFF,
            max_mastering_luminance: 0xFFFF,
            min_mastering_luminance: 0xFFFF,
        }),
        extension: Some(SlHdrExtension {
            extension_6bits: 0x3F,
            data: alloc::vec![0xAB; 1023],
        }),
        sl_hdr_payload_mode: 1,
        matrix_coefficient_values: [0xFFFF; 4],
        chroma_to_luma_injection: [0xFFFF; 2],
        k_coefficient_values: [0xFF; 3],
        payload: SlHdrPayload::Mode1(alloc::boxed::Box::new(SlHdrMode1 {
            lm_uniform_sampling_flag: false,
            luminance_mapping: make_table(),
            cc_uniform_sampling_flag: false,
            colour_correction: make_table(),
        })),
    };

    let meta = SlHdrMetadata {
        itu_t_t35_country_code: 0xB5,
        terminal_provider_code: 0x003C,
        terminal_provider_oriented_code_message_idc: 0x01,
        sl_hdr_mode_value_minus1: 0,
        sl_hdr_spec_major_version_idc: 0,
        sl_hdr_spec_minor_version_idc: 0,
        sl_hdr_cancel_flag: false,
        body: Some(body),
    };

    let frame = DynamicHdrInfoFrame::SlHdr(alloc::boxed::Box::new(meta));
    // Must not panic.
    let _pkts: alloc::vec::Vec<[u8; 31]> = frame.into_packets().value.collect();
}
