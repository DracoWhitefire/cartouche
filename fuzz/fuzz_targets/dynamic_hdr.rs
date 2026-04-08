#![no_main]

use cartouche::dynamic_hdr::{DynamicHdrInfoFrame, Hdr10PlusMetadata, SlHdrMetadata};
use cartouche::encode::IntoPackets;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Exercise Hdr10PlusMetadata::decode directly on arbitrary bytes.
    let _ = Hdr10PlusMetadata::decode(data, &mut |_| {});
    // Exercise SlHdrMetadata::decode directly on arbitrary bytes.
    let _ = SlHdrMetadata::decode(data, &mut |_| {});

    // Treat the fuzz input as a sequence of 31-byte packets.
    let packets: Vec<[u8; 31]> = data
        .chunks(31)
        .map(|chunk| {
            let mut packet = [0u8; 31];
            packet[..chunk.len()].copy_from_slice(chunk);
            packet
        })
        .collect();

    // Round-trip: decode → encode → decode; both decoded values must agree.
    if let Ok(first) = DynamicHdrInfoFrame::decode_sequence(&packets) {
        let re_packets: Vec<[u8; 31]> =
            first.value.clone().into_packets().value.collect();
        if !re_packets.is_empty() {
            if let Ok(second) = DynamicHdrInfoFrame::decode_sequence(&re_packets) {
                assert_eq!(first.value, second.value, "round-trip mismatch");
            }
        }
    }
});
