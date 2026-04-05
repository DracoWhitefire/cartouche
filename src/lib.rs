//! Encoding and decoding for HDMI InfoFrames.
//!
//! `cartouche` encodes and decodes the five HDMI 2.1 InfoFrame types: AVI, Audio,
//! HDR Static Metadata, HDMI Forum Vendor-Specific, and Dynamic HDR. It is a pure
//! encoding/decoding library with no I/O and no allocation requirement.
//!
//! # Features
//!
//! - `std` (default, implies `alloc`): enables `std` support. `Decoded<T, W>` uses
//!   `Vec<W>` for warning storage.
//! - `alloc`: enables `alloc` support without `std`. `Decoded<T, W>` uses `Vec<W>`.
//! - `serde`: derives `Serialize` and `Deserialize` on all public types.
//!
//! Without `alloc` or `std`, warning storage falls back to a fixed `[Option<W>; 8]`
//! array. No other behaviour changes.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(any(feature = "alloc", feature = "std"))]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

/// The [`IntoPackets`](encode::IntoPackets) encoding trait.
pub mod encode;

mod checksum;

/// The [`DecodeError`](error::DecodeError) type.
pub mod error;

/// Per-frame warning enums and the unified [`Warning`](warn::Warning) wrapper.
pub mod warn;
