/// Compute the checksum byte for an InfoFrame packet.
///
/// Takes the 30 bytes that make up the packet excluding the checksum itself
/// (i.e. the 3 header bytes followed by the 27 payload bytes) and returns
/// the value that, when placed at byte 3, makes the sum of all 31 bytes
/// equal to 0x00 mod 256.
///
/// On encode, this is called with the assembled header and payload; the
/// returned byte is written into position 3 of the outgoing packet. On
/// decode, the caller verifies that the sum of all 31 received bytes is
/// zero.
pub(crate) fn compute_checksum(header_and_payload: &[u8; 30]) -> u8 {
    let sum: u8 = header_and_payload
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b));
    0u8.wrapping_sub(sum)
}

#[cfg(test)]
#[path = "checksum_tests.rs"]
mod tests;
