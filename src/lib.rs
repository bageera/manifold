//! Manifold: lossless RGB8 shared-patch-atlas image codecs (MNF5/6/7/8).
//!
//! NOTE on names: the published *package* is `manifold-codec`, but the lib
//! target is `manifold` (the original crate name, kept for path stability).
//! Depend on `manifold-codec` in Cargo.toml and import as `manifold`:
//!
//! ```toml
//! [dependencies]
//! manifold-codec = "1.3"
//! ```
//! ```no_run
//! use manifold::codec::Dims;
//! ```
//! Entry points:
//! - [`codec`]: MNF5 (v0.5 format) encode/decode; [`codec::DecodeLimits`].
//! - [`mnf6`]: MNF6/7 (zstd container, planar, YCoCg, color modes) —
//!   [`mnf6::encode_mnf7`]/[`mnf6::decode_mnf7`] + configurable ceilings
//!   ([`mnf6::decode_mnf7_with_limits`]).
//! - [`mnf8`]: MNF8 (backend byte: zlib/zstd-19, planar, FLAG_PRED_MED,
//!   balanced|maximum profiles) — [`mnf8::encode_mnf8`]/[`mnf8::decode_mnf8`].
//! - [`refdecode`]: independent MNF5 decoder used by the differential tests.
//! - [`error`]: the shared error/`Result` type for every fallible op.
//!
//! Decoding is deterministic integer arithmetic (no float, no allocation
//! beyond the pixel buffer). Formats are frozen at v1.x; malformed input
//! returns [`error::Error::Format`], never a panic.
#![warn(missing_docs)]
/// MNF5 (v0.5 format): atlas codec, zlib container.
pub mod codec;
/// Shared error and `Result` types for the whole library.
pub mod error;
/// Image-I/O shims (JPEG/PNG load, PNG save) — `image-io` feature.
#[cfg(feature = "image-io")]
pub mod img;
/// MNF6/7: zstd container, planar packing, YCoCg + modular color modes. `decode_mnf7` accepts both magics.
pub mod mnf6;
/// MNF8: versioned backend-byte container (zlib/zstd-19), profiles, `FLAG_PRED_MED`.
pub mod mnf8;
/// Independent MNF5 decoder (differential-testing instrument).
pub mod refdecode;
/// LZ77-2D token engine (MNF8 `FLAG_TOKENS`; the retained ladder arm).
pub mod tokens;
