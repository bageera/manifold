// Error taxonomy for the Manifold library surface.
//
// One error type across all decoders (MNF5/6/7/8, refdecode) and encode
// fallibility, replacing the per-module String returns (library hardening:
// downstream consumers can match on the category, not parse messages).
//
// Design constraints from the repo's own rules:
// - Deterministic-integer decoding contract unaffected (errors are about
//   validation, not arithmetic).
// - Panic-free untrusted-input handling: every malformed-input path maps to
//   `Error::Format`, never a panic (audit/2026-10-25 + review/2026-10-25).

use std::fmt;

/// Every fallible library operation returns this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file/stream is structurally invalid for its claimed magic:
    /// bad magic, impossible dims, tile_count mismatch, truncated records,
    /// dangling/shape-mismatched references, invalid transform, reserved
    /// bits/backend set, trailing bytes, decompressed-size violation.
    /// `detail` is human-only (never parsed); `position` is the byte or tile
    /// index where validation failed when known.
    Format {
        /// Human-readable description (never parsed).
        detail: String,
        /// Byte/tile index when known.
        position: Option<u64>,
    },
    /// The stream is structurally plausible but claims more bytes/pixels/
    /// tiles than the configured `DecodeLimits` allow.
    Limits {
        /// Which limit (e.g. `"max_pixels"`).
        which: &'static str,
        /// The configured ceiling.
        limit: u64,
        /// The rejected value.
        actual: u64,
    },
    /// Decompression failed (corrupt zlib/zstd stream, bad checksum).
    Decompress(&'static str),
    /// Container/backend/feature not supported by this build
    /// (reserved backend, feature-gated coder compiled out).
    Unsupported {
        /// `what` names the unshipped/reserved feature.
        what: String,
    },
    /// I/O failure reading the input or writing the output PNG.
    Io(String),
    /// Image I/O (decode-to-RGB8 of a JPEG/PNG source, PNG encode).
    ImageIo(String),
    /// Internal invariant that the library treats as unreachable (should
    /// never surface to callers; a bug if it does).
    Internal(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Format { detail, position } => match position {
                Some(p) => write!(f, "invalid stream: {detail} (at {p})"),
                None => write!(f, "invalid stream: {detail}"),
            },
            Error::Limits {
                which,
                limit,
                actual,
            } => {
                write!(f, "{which} limit exceeded: {actual} > {limit}")
            }
            Error::Decompress(what) => write!(f, "decompress: {what}"),
            Error::Unsupported { what } => write!(f, "unsupported: {what}"),
            Error::Io(e) => write!(f, "i/o: {e}"),
            Error::ImageIo(e) => write!(f, "image: {e}"),
            Error::Internal(what) => write!(f, "internal error: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

/// Shorthand alias for library results.
pub type Result<T> = std::result::Result<T, Error>;

// Internal constructors (keep call sites terse; the lib never exposes
// constructing Format-detail strings as public API in v1.x).

impl Error {
    pub(crate) fn fmt_at(detail: impl fmt::Display, position: u64) -> Error {
        Error::Format {
            detail: detail.to_string(),
            position: Some(position),
        }
    }

    /// Public constructor: a format/descriptive error from an explicit string.
    pub fn msg(detail: impl fmt::Display) -> Error {
        Error::Format {
            detail: detail.to_string(),
            position: None,
        }
    }

    pub(crate) fn fmt_msg(detail: impl fmt::Display) -> Error {
        Error::msg(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_stable() {
        let e = Error::fmt_at("tile 3 truncated", 3);
        assert_eq!(e.to_string(), "invalid stream: tile 3 truncated (at 3)");
        let e = Error::Limits {
            which: "pixels",
            limit: 67_108_864,
            actual: 67_108_865,
        };
        assert_eq!(e.to_string(), "pixels limit exceeded: 67108865 > 67108864");
    }

    #[test]
    fn error_is_std_error() {
        fn assert_error<T: std::error::Error>(_: &T) {}
        assert_error(&Error::Decompress("x"));
    }
}
