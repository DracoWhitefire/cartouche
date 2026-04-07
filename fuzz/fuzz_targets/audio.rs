#![no_main]

use cartouche::audio::AudioInfoFrame;
use cartouche::encode::IntoPackets;
use cartouche::error::DecodeError;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut packet = [0u8; 31];
    let n = data.len().min(31);
    packet[..n].copy_from_slice(&data[..n]);

    match AudioInfoFrame::decode(&packet) {
        Ok(decoded) => {
            let re_packet = decoded.value.clone().into_packets().value.next().unwrap();
            let re_decoded = AudioInfoFrame::decode(&re_packet).unwrap();
            assert_eq!(re_decoded.value, decoded.value);
        }
        Err(DecodeError::Truncated { .. }) | Err(_) => {}
    }
});
