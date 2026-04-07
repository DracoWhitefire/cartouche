#![no_main]

use cartouche::avi::AviInfoFrame;
use cartouche::encode::IntoPackets;
use cartouche::error::DecodeError;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut packet = [0u8; 31];
    let n = data.len().min(31);
    packet[..n].copy_from_slice(&data[..n]);

    match AviInfoFrame::decode(&packet) {
        Ok(decoded) => {
            // Round-trip: re-encoding a decoded frame and re-decoding must yield
            // the same value. The checksum is always recomputed on encode, so the
            // re-encoded packet has a valid checksum and no ChecksumMismatch warning.
            let re_packet = decoded.value.clone().into_packets().next().unwrap();
            let re_decoded = AviInfoFrame::decode(&re_packet).unwrap();
            assert_eq!(re_decoded.value, decoded.value);
        }
        Err(DecodeError::Truncated { .. }) | Err(_) => {}
    }
});
