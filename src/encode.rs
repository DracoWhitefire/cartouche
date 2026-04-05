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
/// # Example
///
/// ```rust
/// # use cartouche::encode::IntoPackets;
/// # fn transmit(_: &[u8; 31]) {}
/// # fn example<F: IntoPackets>(frame: F) {
/// for packet in frame.into_packets() {
///     transmit(&packet);
/// }
/// # }
/// ```
pub trait IntoPackets {
    /// The iterator type that yields wire packets.
    type Iter: Iterator<Item = [u8; 31]>;

    /// Consume the frame and return an iterator over its wire packets.
    fn into_packets(self) -> Self::Iter;
}
