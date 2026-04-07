/// Warnings produced while decoding an AVI InfoFrame.
///
/// A warning indicates an anomaly in the packet that does not prevent decoding.
/// The frame is returned alongside any warnings; the caller decides how to act
/// on them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AviWarning {
    /// The packet checksum did not verify.
    ///
    /// `expected` is the checksum byte value that would have made the sum of
    /// all 31 bytes equal zero; `found` is the value actually present in the
    /// packet.
    ChecksumMismatch {
        /// The checksum value that would have verified correctly.
        expected: u8,
        /// The checksum value found in the packet.
        found: u8,
    },
    /// A reserved field was set to a non-zero value.
    ///
    /// `byte` is the zero-indexed byte position within the 31-byte packet;
    /// `bit` is the zero-indexed bit position within that byte.
    ReservedFieldNonZero {
        /// Zero-indexed byte position within the packet.
        byte: u8,
        /// Zero-indexed bit position within the byte.
        bit: u8,
    },
    /// A field contained a value not defined in the specification.
    ///
    /// `field` names the field (e.g. `"colorimetry"`); `raw` is the raw byte
    /// value that was not recognised.
    UnknownEnumValue {
        /// Name of the field that contained the unrecognised value.
        field: &'static str,
        /// The raw byte value that was not recognised.
        raw: u8,
    },
}

/// Warnings produced while decoding an Audio InfoFrame.
///
/// See [`AviWarning`] for variant documentation — the variants are identical
/// across all per-frame warning types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AudioWarning {
    /// The packet checksum did not verify.
    ChecksumMismatch {
        /// The checksum value that would have verified correctly.
        expected: u8,
        /// The checksum value found in the packet.
        found: u8,
    },
    /// A reserved field was set to a non-zero value.
    ReservedFieldNonZero {
        /// Zero-indexed byte position within the packet.
        byte: u8,
        /// Zero-indexed bit position within the byte.
        bit: u8,
    },
    /// A field contained a value not defined in the specification.
    UnknownEnumValue {
        /// Name of the field that contained the unrecognised value.
        field: &'static str,
        /// The raw byte value that was not recognised.
        raw: u8,
    },
}

/// Warnings produced while decoding an HDR Static Metadata InfoFrame.
///
/// See [`AviWarning`] for variant documentation — the variants are identical
/// across all per-frame warning types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HdrStaticWarning {
    /// The packet checksum did not verify.
    ChecksumMismatch {
        /// The checksum value that would have verified correctly.
        expected: u8,
        /// The checksum value found in the packet.
        found: u8,
    },
    /// A reserved field was set to a non-zero value.
    ReservedFieldNonZero {
        /// Zero-indexed byte position within the packet.
        byte: u8,
        /// Zero-indexed bit position within the byte.
        bit: u8,
    },
    /// A field contained a value not defined in the specification.
    UnknownEnumValue {
        /// Name of the field that contained the unrecognised value.
        field: &'static str,
        /// The raw byte value that was not recognised.
        raw: u8,
    },
}

/// Warnings produced while decoding an HDMI Forum Vendor-Specific InfoFrame.
///
/// See [`AviWarning`] for variant documentation — the variants are identical
/// across all per-frame warning types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HdmiForumVsiWarning {
    /// The packet checksum did not verify.
    ChecksumMismatch {
        /// The checksum value that would have verified correctly.
        expected: u8,
        /// The checksum value found in the packet.
        found: u8,
    },
    /// A reserved field was set to a non-zero value.
    ReservedFieldNonZero {
        /// Zero-indexed byte position within the packet.
        byte: u8,
        /// Zero-indexed bit position within the byte.
        bit: u8,
    },
    /// A field contained a value not defined in the specification.
    UnknownEnumValue {
        /// Name of the field that contained the unrecognised value.
        field: &'static str,
        /// The raw byte value that was not recognised.
        raw: u8,
    },
}

/// Warnings produced while decoding a Dynamic HDR InfoFrame.
///
/// See [`AviWarning`] for variant documentation — the variants are identical
/// across all per-frame warning types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DynamicHdrWarning {
    /// The packet checksum did not verify.
    ChecksumMismatch {
        /// The checksum value that would have verified correctly.
        expected: u8,
        /// The checksum value found in the packet.
        found: u8,
    },
    /// A reserved field was set to a non-zero value.
    ReservedFieldNonZero {
        /// Zero-indexed byte position within the packet.
        byte: u8,
        /// Zero-indexed bit position within the byte.
        bit: u8,
    },
    /// A field contained a value not defined in the specification.
    UnknownEnumValue {
        /// Name of the field that contained the unrecognised value.
        field: &'static str,
        /// The raw byte value that was not recognised.
        raw: u8,
    },
}

/// A warning produced by the top-level [`decode`](crate::decode) function.
///
/// When decoding a single packet without knowing its type in advance, the
/// top-level dispatch returns this unified warning type. Each variant wraps
/// the corresponding per-frame warning; match on the variant to obtain the
/// inner type.
///
/// Callers that decode a specific InfoFrame type directly (e.g.
/// [`AviInfoFrame::decode`](crate::avi::AviInfoFrame::decode)) receive the
/// per-frame warning type and do not need this wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// A warning from decoding an AVI InfoFrame.
    Avi(AviWarning),
    /// A warning from decoding an Audio InfoFrame.
    Audio(AudioWarning),
    /// A warning from decoding an HDR Static Metadata InfoFrame.
    HdrStatic(HdrStaticWarning),
    /// A warning from decoding an HDMI Forum Vendor-Specific InfoFrame.
    HdmiForumVsi(HdmiForumVsiWarning),
    /// A warning from decoding a Dynamic HDR InfoFrame.
    DynamicHdr(DynamicHdrWarning),
}
