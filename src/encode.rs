/// A one-shot iterator that yields a single 31-byte wire packet.
///
/// Used as the [`IntoPackets::Iter`] type for all single-packet InfoFrame
/// types (AVI, Audio, HDR Static Metadata, HDMI Forum VSI).
pub struct SinglePacketIter(Option<[u8; 31]>);

impl SinglePacketIter {
    pub(crate) fn new(packet: [u8; 31]) -> Self {
        Self(Some(packet))
    }
}

impl Iterator for SinglePacketIter {
    type Item = [u8; 31];

    fn next(&mut self) -> Option<[u8; 31]> {
        self.0.take()
    }
}

/// Yields one or more 31-byte wire packets representing an InfoFrame.
///
/// Traditional InfoFrame types (AVI, Audio, HDR Static Metadata, HDMI Forum VSI)
/// yield exactly one packet. Dynamic HDR yields as many packets as the metadata
/// payload requires.
///
/// The iterator owns the frame — `into_packets` moves `self`. Callers that need
/// to retain the frame after encoding should clone before calling.
///
/// # Warnings
///
/// Like the decode path, `into_packets` returns a [`Decoded`] that pairs the
/// packet iterator with any warnings produced during encoding. Callers should
/// check [`Decoded::iter_warnings`] before transmitting packets.
///
/// # Example
///
/// ```rust
/// # use cartouche::encode::IntoPackets;
/// # fn transmit(_: &[u8; 31]) {}
/// # fn example<F: IntoPackets>(frame: F) {
/// let encoded = frame.into_packets();
/// // (check encoded.iter_warnings() here)
/// for packet in encoded.value {
///     transmit(&packet);
/// }
/// # }
/// ```
pub trait IntoPackets {
    /// The iterator type that yields wire packets.
    type Iter: Iterator<Item = [u8; 31]>;
    /// The warning type produced during encoding.
    type Warning;

    /// Consume the frame and return a [`Decoded`] containing the packet
    /// iterator and any encode-time warnings.
    fn into_packets(self) -> crate::decoded::Decoded<Self::Iter, Self::Warning>;
}
