use crate::decoded::Decoded;
pub use crate::dynamic_hdr::hdr10plus::Hdr10PlusMetadata;
pub use crate::dynamic_hdr::slhdr::SlHdrMetadata;
use crate::encode::IntoPackets;
use crate::error::DecodeError;
use crate::warn::DynamicHdrWarning;

/// Maximum byte length of an assembled Dynamic HDR metadata payload.
///
/// Sized for a worst-case SL-HDR mode-1 + max extension fields populated: approximately 2107 bytes.
/// Used as the stack-buffer size in bare `no_std` builds where heap allocation is unavailable.
pub(crate) const MAX_DYNAMIC_HDR_PAYLOAD: usize = 2200;

/// MSB-first bit-stream reader.
///
/// Used to parse HDR10+ and SL-HDR payloads, whose fields are bit-packed
/// with no byte alignment (ETSI TS 103 433-1 §6.1).
struct BitReader<'a> {
    data: &'a [u8],
    /// Index of the byte currently being read.
    byte_pos: usize,
    /// Next bit to read within `data[byte_pos]`, counting from the MSB.
    /// 0 = MSB (bit 7), 7 = LSB (bit 0). After bit 7 the reader advances to
    /// the next byte.
    bit_pos: u8,
}
impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read `bits` bits (1–8) and return them right-aligned in a `u8`.
    fn read_u8(&mut self, bits: u8) -> Result<u8, DecodeError> {
        debug_assert!((1..=8).contains(&bits));
        Ok(self.read_bits(bits)? as u8)
    }

    /// Read `bits` bits (1–16) and return them right-aligned in a `u16`.
    fn read_u16(&mut self, bits: u8) -> Result<u16, DecodeError> {
        debug_assert!((1..=16).contains(&bits));
        Ok(self.read_bits(bits)? as u16)
    }

    /// Read `bits` bits (1–32) and return them right-aligned in a `u32`.
    fn read_u32(&mut self, bits: u8) -> Result<u32, DecodeError> {
        debug_assert!((1..=32).contains(&bits));
        self.read_bits(bits)
    }

    /// Read a single bit as a `bool`.
    fn read_bool(&mut self) -> Result<bool, DecodeError> {
        Ok(self.read_bits(1)? != 0)
    }

    /// Number of bits remaining in the buffer.
    fn remaining_bits(&self) -> usize {
        let remaining_bytes = self.data.len().saturating_sub(self.byte_pos);
        remaining_bytes * 8 - self.bit_pos as usize
    }

    /// Core read: consumes `n` bits MSB-first and returns them in the low bits of a `u32`.
    fn read_bits(&mut self, mut n: u8) -> Result<u32, DecodeError> {
        if self.remaining_bits() < n as usize {
            return Err(DecodeError::MalformedPayload);
        }
        let mut result: u32 = 0;
        while n > 0 {
            // Bits available in the current byte.
            let avail = 8 - self.bit_pos;
            let take = n.min(avail);
            // Shift the current byte so the next `take` bits are at the top,
            // then mask them off.
            let shift = avail - take;
            let mask = ((1u16 << take) - 1) as u8;
            let bits = (self.data[self.byte_pos] >> shift) & mask;
            result = (result << take) | bits as u32;
            self.bit_pos += take;
            if self.bit_pos == 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
            n -= take;
        }
        Ok(result)
    }
}

/// MSB-first bit-stream writer.
///
/// Packs fields into a fixed-size stack buffer for encoding HDR10+ and SL-HDR
/// payloads. Panics on overflow — callers must not exceed
/// `MAX_DYNAMIC_HDR_PAYLOAD` bytes.
struct BitWriter {
    buf: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    /// Index of the byte currently being written.
    byte_pos: usize,
    /// Next bit to write within `buf[byte_pos]`, counting from the MSB.
    /// 0 = MSB (bit 7), 7 = LSB (bit 0).
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            buf: [0u8; MAX_DYNAMIC_HDR_PAYLOAD],
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Write `bits` bits (1–8) from the low bits of `value`.
    fn write_u8(&mut self, value: u8, bits: u8) {
        debug_assert!((1..=8).contains(&bits));
        self.write_bits(value as u32, bits);
    }

    /// Write `bits` bits (1–16) from the low bits of `value`.
    fn write_u16(&mut self, value: u16, bits: u8) {
        debug_assert!((1..=16).contains(&bits));
        self.write_bits(value as u32, bits);
    }

    /// Write `bits` bits (1–32) from the low bits of `value`.
    fn write_u32(&mut self, value: u32, bits: u8) {
        debug_assert!((1..=32).contains(&bits));
        self.write_bits(value, bits);
    }

    /// Write a single bit.
    fn write_bool(&mut self, value: bool) {
        self.write_bits(value as u32, 1);
    }

    /// Returns the populated slice of the buffer.
    fn finish(self) -> ([u8; MAX_DYNAMIC_HDR_PAYLOAD], usize) {
        let len = if self.bit_pos == 0 {
            self.byte_pos
        } else {
            self.byte_pos + 1
        };
        (self.buf, len)
    }

    /// Core write: places the low `n` bits of `value` into the buffer MSB-first.
    fn write_bits(&mut self, value: u32, mut n: u8) {
        assert!(
            self.byte_pos < MAX_DYNAMIC_HDR_PAYLOAD,
            "BitWriter overflow"
        );
        while n > 0 {
            let avail = 8 - self.bit_pos;
            let take = n.min(avail);
            // Extract the top `take` bits from the remaining `n` bits of value.
            let shift = n - take;
            let bits = ((value >> shift) as u8) & (((1u16 << take) - 1) as u8);
            // Place them at the correct position within the current byte.
            self.buf[self.byte_pos] |= bits << (avail - take);
            self.bit_pos += take;
            if self.bit_pos == 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
            n -= take;
        }
    }
}

/// A Dynamic HDR InfoFrame.
///
/// Carries per-frame or per-scene dynamic tone mapping metadata for formats
/// including HDR10+ (ETSI TS 103 433) and SL-HDR. Unlike all other InfoFrame
/// types, the payload is variable length and spans multiple packets.
///
/// Use [`DynamicHdrFragment::decode`] to decode individual packets as they
/// arrive. Once the full sequence is assembled, pass the raw packets to
/// [`DynamicHdrInfoFrame::decode_sequence`] to obtain this type.
///
/// # Stack size
///
/// The `Hdr10Plus` variant is approximately 1,700 bytes and `SlHdr`
/// approximately 1,100 bytes. Boxing would require `alloc`, so the metadata
/// is stored inline in all build configurations. Callers who need a
/// pointer-sized handle can box the whole `DynamicHdrInfoFrame` at the call
/// site; callers on targets with limited stack may wish to store it in a
/// `static`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DynamicHdrInfoFrame {
    /// HDR10+ dynamic metadata (ETSI TS 103 433-1, format identifier `0x04`).
    Hdr10Plus(Hdr10PlusMetadata),
    /// SL-HDR dynamic metadata (ETSI TS 103 433-1 Table A.1, format identifier
    /// `0x02`).
    SlHdr(SlHdrMetadata),
    /// An unrecognised metadata format.
    ///
    /// Returned when the format identifier in the packet sequence is not
    /// recognised by this version of `cartouche`. In `alloc`/`std` builds the
    /// raw metadata bytes are retained in `payload`, making this variant
    /// re-encodable. In bare `no_std` builds the payload is not retained.
    Unknown {
        /// The metadata format identifier from the first packet in the sequence.
        format_id: u8,
        /// Raw metadata bytes concatenated from all chunks in the sequence.
        ///
        /// Only present in `alloc`/`std` builds. In bare `no_std` builds the
        /// payload is not retained.
        #[cfg(any(feature = "alloc", feature = "std"))]
        payload: alloc::vec::Vec<u8>,
    },
}

impl DynamicHdrInfoFrame {
    /// Assemble a [`DynamicHdrInfoFrame`] from a complete sequence of wire packets.
    ///
    /// The caller is responsible for collecting the sequence. When the sum of
    /// `chunk_len` values across all [`DynamicHdrFragment`]s received via the
    /// top-level [`decode`](crate::decode) function equals `total_bytes`, the
    /// sequence is complete and ready to pass here.
    ///
    /// The format identifier is read from the first packet in the sequence
    /// (byte 7, PB3). Unknown format identifiers produce
    /// [`DynamicHdrInfoFrame::Unknown`]; in bare `no_std` builds the raw
    /// payload bytes are not retained.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if any packet in `packets` has
    /// `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`] — for any packet whose
    ///   checksum does not verify.
    pub fn decode_sequence(
        packets: &[[u8; 31]],
    ) -> Result<Decoded<DynamicHdrInfoFrame, DynamicHdrWarning>, DecodeError> {
        if packets.is_empty() {
            return Err(DecodeError::EmptySequence);
        }

        // Read sequence-level invariants from the first packet.
        let format_id = packets[0][7];
        let total_bytes = u16::from_le_bytes([packets[0][5], packets[0][6]]);

        let mut decoded = Decoded::new(DynamicHdrInfoFrame::Unknown {
            format_id,
            #[cfg(any(feature = "alloc", feature = "std"))]
            payload: alloc::vec::Vec::new(),
        });

        // Accumulate metadata bytes from all packets into a fixed-size stack
        // buffer. MAX_DYNAMIC_HDR_PAYLOAD is sized for the worst-case payload.
        let mut payload_buf = [0u8; MAX_DYNAMIC_HDR_PAYLOAD];
        let mut payload_len = 0usize;

        for (i, packet) in packets.iter().enumerate() {
            let frag = DynamicHdrFragment::decode(packet)?;
            for w in frag.iter_warnings() {
                decoded.push_warning(w.clone());
            }

            // Sequence integrity.
            if frag.value.seq_num != i as u8 {
                decoded.push_warning(DynamicHdrWarning::OutOfOrderPacket {
                    index: i as u8,
                    found: frag.value.seq_num,
                });
            }
            // Consistency against the first packet's invariants (skip i==0: trivially equal).
            if i > 0 {
                if frag.value.total_bytes != total_bytes {
                    decoded.push_warning(DynamicHdrWarning::InconsistentTotalBytes {
                        packet: i as u8,
                        expected: total_bytes,
                        found: frag.value.total_bytes,
                    });
                }
                if frag.value.format_id != format_id {
                    decoded.push_warning(DynamicHdrWarning::InconsistentFormatId {
                        packet: i as u8,
                        expected: format_id,
                        found: frag.value.format_id,
                    });
                }
            }

            // Chunk accumulation.
            let chunk = &frag.value.chunk[..frag.value.chunk_len as usize];
            let new_len = payload_len.saturating_add(chunk.len());
            if new_len <= MAX_DYNAMIC_HDR_PAYLOAD {
                payload_buf[payload_len..new_len].copy_from_slice(chunk);
                payload_len = new_len;
            }
            // If the payload overflows MAX_DYNAMIC_HDR_PAYLOAD the buffer is
            // silently capped; a MalformedPayload error from the format parser
            // will surface the truncation.
        }

        let payload = &payload_buf[..payload_len];

        // Collect format-level warnings into a small fixed buffer so the
        // closure does not need to borrow `decoded` during dispatch.
        let mut fmt_warn_count = 0usize;
        let mut fmt_warns: [Option<DynamicHdrWarning>; 4] = [const { None }; 4];
        let decoded_value = {
            let mut push_fmt_warn = |w: DynamicHdrWarning| {
                if fmt_warn_count < 4 {
                    fmt_warns[fmt_warn_count] = Some(w);
                    fmt_warn_count += 1;
                }
            };
            match format_id {
                0x02 => match SlHdrMetadata::decode(payload, &mut push_fmt_warn) {
                    Ok(meta) => DynamicHdrInfoFrame::SlHdr(meta),
                    Err(e) => return Err(e),
                },
                0x04 => match Hdr10PlusMetadata::decode(payload, &mut push_fmt_warn) {
                    Ok(meta) => DynamicHdrInfoFrame::Hdr10Plus(meta),
                    Err(e) => return Err(e),
                },
                _ => {
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    {
                        DynamicHdrInfoFrame::Unknown {
                            format_id,
                            payload: payload.to_vec(),
                        }
                    }
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    {
                        DynamicHdrInfoFrame::Unknown { format_id }
                    }
                }
            }
        };
        decoded.value = decoded_value;
        for w in fmt_warns[..fmt_warn_count]
            .iter_mut()
            .filter_map(|opt| opt.take())
        {
            decoded.push_warning(w);
        }

        Ok(decoded)
    }
}

/// Iterator that yields 31-byte wire packets for a [`DynamicHdrInfoFrame`].
///
/// Produced by [`DynamicHdrInfoFrame::into_packets`]. Yields one packet per
/// 23-byte chunk of the serialized metadata payload; the final packet carries
/// any remaining bytes (fewer than 23).
pub struct DynamicHdrIter {
    format_id: u8,
    total_bytes: u16,
    offset: usize,
    seq_num: u8,
    #[cfg(any(feature = "alloc", feature = "std"))]
    payload: alloc::vec::Vec<u8>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload: [u8; MAX_DYNAMIC_HDR_PAYLOAD],
    /// Length of valid bytes in `payload` (bare `no_std` builds only).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    payload_len: usize,
}

impl Iterator for DynamicHdrIter {
    type Item = [u8; 31];

    fn next(&mut self) -> Option<[u8; 31]> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        let payload_len = self.payload.len();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        let payload_len = self.payload_len;

        if self.offset >= payload_len {
            return None;
        }

        let chunk_len = (payload_len - self.offset).min(23);

        // Build the 30 non-checksum bytes: [type, version, length, PB0..PB26].
        // Byte layout (offsets in the final 31-byte packet):
        //   0: type_code=0x20, 1: version=0x01, 2: length=4+chunk_len
        //   3: checksum (filled below), 4: seq_num, 5–6: total_bytes LE,
        //   7: format_id, 8..8+chunk_len: chunk data
        let mut hp = [0u8; 30];
        hp[0] = 0x20; // Dynamic HDR type code
        hp[1] = 0x01; // version
        hp[2] = (4 + chunk_len) as u8;
        hp[3] = self.seq_num;
        let tb = self.total_bytes.to_le_bytes();
        hp[4] = tb[0];
        hp[5] = tb[1];
        hp[6] = self.format_id;
        hp[7..7 + chunk_len].copy_from_slice(&self.payload[self.offset..self.offset + chunk_len]);

        let checksum = crate::checksum::compute_checksum(&hp);

        let mut packet = [0u8; 31];
        packet[..3].copy_from_slice(&hp[..3]);
        packet[3] = checksum;
        packet[4..].copy_from_slice(&hp[3..]);

        self.offset += chunk_len;
        self.seq_num += 1;

        Some(packet)
    }
}

impl IntoPackets for DynamicHdrInfoFrame {
    type Iter = DynamicHdrIter;
    type Warning = DynamicHdrWarning;

    fn into_packets(self) -> Decoded<DynamicHdrIter, DynamicHdrWarning> {
        match self {
            DynamicHdrInfoFrame::Unknown {
                format_id,
                #[cfg(any(feature = "alloc", feature = "std"))]
                payload,
            } => {
                #[cfg(any(feature = "alloc", feature = "std"))]
                let total_bytes = payload.len() as u16;
                #[cfg(not(any(feature = "alloc", feature = "std")))]
                let total_bytes: u16 = 0;

                Decoded::new(DynamicHdrIter {
                    format_id,
                    total_bytes,
                    offset: 0,
                    seq_num: 0,
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    payload,
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload: [0u8; MAX_DYNAMIC_HDR_PAYLOAD],
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload_len: 0,
                })
            }
            DynamicHdrInfoFrame::Hdr10Plus(meta) => {
                let (buf, len) = meta.encode();
                Decoded::new(DynamicHdrIter {
                    format_id: 0x04,
                    total_bytes: len as u16,
                    offset: 0,
                    seq_num: 0,
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    payload: buf[..len].to_vec(),
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload: buf,
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload_len: len,
                })
            }
            DynamicHdrInfoFrame::SlHdr(meta) => {
                let (buf, len) = meta.encode();
                Decoded::new(DynamicHdrIter {
                    format_id: 0x02,
                    total_bytes: len as u16,
                    offset: 0,
                    seq_num: 0,
                    #[cfg(any(feature = "alloc", feature = "std"))]
                    payload: buf[..len].to_vec(),
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload: buf,
                    #[cfg(not(any(feature = "alloc", feature = "std")))]
                    payload_len: len,
                })
            }
        }
    }
}

/// A single packet's worth of Dynamic HDR metadata, as returned by the
/// top-level [`decode`](crate::decode) function.
///
/// A full [`DynamicHdrInfoFrame`] cannot be assembled from a single wire
/// packet. The top-level decode path therefore returns this fragment type,
/// which exposes the fields the caller needs to accumulate a complete sequence.
/// Once all packets in the sequence have been collected, pass them to
/// `DynamicHdrInfoFrame::decode_sequence` to assemble the full frame.
///
/// # Wire layout
///
/// ```text
/// Byte 4 (PB0):    seq_num    — packet index in sequence (0-based)
/// Bytes 5–6 (PB1–2): total_bytes — total metadata byte count (little-endian u16)
/// Byte 7 (PB3):    format_id  — metadata format identifier
/// Bytes 8–30 (PB4–26): chunk  — up to 23 metadata bytes
/// ```
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
    /// packet in a sequence may carry fewer than 23 bytes.
    pub chunk: [u8; 23],
    /// Number of valid bytes in [`chunk`](DynamicHdrFragment::chunk).
    ///
    /// Always ≤ 23.
    pub chunk_len: u8,
}

impl DynamicHdrFragment {
    /// Decode a single Dynamic HDR InfoFrame packet into a fragment.
    ///
    /// Each Dynamic HDR packet carries a sequence index, the total metadata
    /// byte count, a format identifier, and up to 23 bytes of metadata. The
    /// caller is responsible for collecting fragments until the sequence is
    /// complete, then passing the full packet sequence to
    /// `DynamicHdrInfoFrame::decode_sequence`.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if `packet[2] > 27`.
    ///
    /// # Warnings
    ///
    /// The returned [`Decoded`] may carry:
    /// - [`DynamicHdrWarning::ChecksumMismatch`]
    pub fn decode(
        packet: &[u8; 31],
    ) -> Result<Decoded<DynamicHdrFragment, DynamicHdrWarning>, DecodeError> {
        let length = packet[2];
        if length > 27 {
            return Err(DecodeError::Truncated { claimed: length });
        }

        let mut decoded = Decoded::new(DynamicHdrFragment {
            seq_num: 0,
            total_bytes: 0,
            format_id: 0,
            chunk: [0u8; 23],
            chunk_len: 0,
        });

        // Checksum verification.
        let total: u8 = packet.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0x00 {
            let expected = crate::checksum::compute_checksum(packet[..30].try_into().unwrap());
            decoded.push_warning(DynamicHdrWarning::ChecksumMismatch {
                expected,
                found: packet[3],
            });
        }

        decoded.value.seq_num = packet[4];
        decoded.value.total_bytes = u16::from_le_bytes([packet[5], packet[6]]);
        decoded.value.format_id = packet[7];

        // chunk_len = payload bytes after the 4-byte overhead, capped at 23.
        let chunk_len = length.saturating_sub(4).min(23);
        decoded.value.chunk_len = chunk_len;
        decoded.value.chunk[..chunk_len as usize]
            .copy_from_slice(&packet[8..8 + chunk_len as usize]);

        Ok(decoded)
    }
}

mod hdr10plus;
mod slhdr;
#[cfg(test)]
mod tests;
