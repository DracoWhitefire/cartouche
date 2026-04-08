/// A hard decode failure.
///
/// Most anomalies encountered while decoding — bad checksum, reserved fields set,
/// unrecognised enum values — are reported as warnings on the returned frame rather
/// than errors. `DecodeError` is returned only when the packet cannot be decoded at
/// all.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The `length` field in the packet header exceeds the available payload capacity.
    ///
    /// A 31-byte packet can hold at most 27 payload bytes (bytes 4–30). If the header
    /// `length` field declares more than 27, the packet cannot be decoded.
    ///
    /// `claimed` is the value of the `length` field. The available payload
    /// capacity of a 31-byte HDMI packet is always 27 bytes (bytes 4–30).
    Truncated {
        /// The value declared in the packet header's `length` field.
        claimed: u8,
    },
    /// [`DynamicHdrInfoFrame::decode_sequence`](crate::dynamic_hdr::DynamicHdrInfoFrame::decode_sequence)
    /// was called with an empty packet slice.
    ///
    /// A Dynamic HDR sequence always contains at least one packet. An empty slice
    /// indicates a caller error: the sequence has not been fully collected yet, or
    /// no packets were received.
    EmptySequence,
    /// The assembled metadata payload is too short to contain the fields
    /// required by its declared format.
    ///
    /// Distinct from [`Truncated`](DecodeError::Truncated), which fires when an
    /// individual packet's `length` header field is out of range. This variant
    /// fires when all packets are individually well-formed but the concatenated
    /// payload bytes run out before all mandatory fields have been read.
    MalformedPayload,
}
