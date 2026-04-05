/// A hard decode failure.
///
/// Most anomalies encountered while decoding — bad checksum, reserved fields set,
/// unrecognised enum values — are reported as warnings on the returned frame rather
/// than errors. `DecodeError` is returned only when the packet cannot be decoded at
/// all.
///
/// The only current variant is [`DecodeError::Truncated`], which fires when the
/// `length` field in the packet header claims more payload bytes than the 31-byte
/// packet can hold (i.e. `length > 27`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The `length` field in the packet header exceeds the available payload capacity.
    ///
    /// A 31-byte packet can hold at most 27 payload bytes (bytes 4–30). If the header
    /// `length` field declares more than 27, the packet cannot be decoded.
    ///
    /// `claimed` is the value of the `length` field; `available` is always 27.
    Truncated {
        /// The value declared in the packet header's `length` field.
        claimed: u8,
        /// The number of payload bytes actually available in the packet (always 27).
        available: u8,
    },
}
