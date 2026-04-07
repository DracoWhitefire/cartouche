#![no_main]

use cartouche::dynamic_hdr::DynamicHdrInfoFrame;
use cartouche::error::DecodeError;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Treat the fuzz input as a sequence of 31-byte packets.
    let packets: Vec<[u8; 31]> = data
        .chunks(31)
        .map(|chunk| {
            let mut packet = [0u8; 31];
            packet[..chunk.len()].copy_from_slice(chunk);
            packet
        })
        .collect();

    match DynamicHdrInfoFrame::decode_sequence(&packets) {
        Ok(_) => {}
        Err(DecodeError::Truncated { .. }) | Err(_) => {}
    }
});
