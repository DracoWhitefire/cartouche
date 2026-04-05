/// A Dynamic HDR InfoFrame.
///
/// Carries per-frame or per-scene dynamic tone mapping metadata for formats
/// including HDR10+ and SL-HDR. Unlike all other InfoFrame types, the payload
/// is variable length and spans multiple packets.
///
/// Fields and encode/decode support are added in a subsequent implementation
/// phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DynamicHdrInfoFrame {}

/// A single packet's worth of Dynamic HDR metadata, as returned by the
/// top-level [`decode`](crate::decode) function.
///
/// A full [`DynamicHdrInfoFrame`] cannot be assembled from a single wire
/// packet. The top-level decode path therefore returns this fragment type,
/// which exposes the fields the caller needs to accumulate a complete sequence.
/// Once all packets in the sequence have been collected, pass them to
/// `DynamicHdrInfoFrame::decode_sequence` to assemble the full frame.
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
    /// packet in a sequence may carry fewer than 29 bytes; all other packets
    /// carry exactly 29.
    pub chunk: [u8; 29],
    /// Number of valid bytes in [`chunk`](DynamicHdrFragment::chunk).
    ///
    /// Always ≤ 29.
    pub chunk_len: u8,
}
