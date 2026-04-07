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
mod tests {
    use super::*;

    #[test]
    fn checksum_makes_total_zero() {
        let header_and_payload = [
            0x82, 0x02, 0x0D, // header bytes (type, version, length)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // payload
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let checksum = compute_checksum(&header_and_payload);
        let sum: u8 = header_and_payload
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b))
            .wrapping_add(checksum);
        assert_eq!(sum, 0x00);
    }

    #[test]
    fn all_zeros_gives_zero_checksum() {
        assert_eq!(compute_checksum(&[0u8; 30]), 0x00);
    }

    #[test]
    fn known_value() {
        // sum of [0x82, 0x02, 0x0D, rest zeros] = 0x82 + 0x02 + 0x0D = 0x91
        // checksum = 256 - 0x91 = 0x6F
        let mut bytes = [0u8; 30];
        bytes[0] = 0x82;
        bytes[1] = 0x02;
        bytes[2] = 0x0D;
        assert_eq!(compute_checksum(&bytes), 0x6F);
    }
}
