//! GSE-10: analytical query engine over the GSE-09 v1.1 recursive-region
//! wire format (research crate, `publish = false`).
//!
//! Format: `docs/gse09/wire-spec.md` (FROZEN — this crate never changes
//! the wire; it adds evaluators and exception indexes on top).
//!
//! Central hypothesis (GSE-10 v1.1 spec §7): the compact mathematical
//! representation must answer region sums competitively against a
//! native Rust direct scan WITHOUT decoding the whole dataset, while
//! the storage-selection contract (GQ2/GQ4) holds unchanged.
//!
//! Normative contract recap (v1.1 amendments — see wire-spec §Exception
//! contract): wire stores replacement bytes v_p; corrections
//! (v_p − M(p)) are derived at query time in SIGNED arithmetic after
//! the generator's mod-256 reduction. Storing a mod-256 delta is a
//! violation; the tests must and do assert this.
//!
//! Gates (pending, per the preregistered matrix):
//! - GQ1 median analytical latency ≤ native Rust scan, full-frame AND
//!   8×8, mixed fixture; NumPy timings = supplementary only.
//! - GQ2 standalone storage accounting per fixture on the complete
//!   64 KiB matrix (losses reported even when the portfolio rejects).
//! - GQ3 sums checked against brute-force over INDEPENDENTLY decoded
//!   wire bytes (no encoder-side working data).
//! - GQ4 complete serialized cost (type+params+exceptions+container),
//!   portfolio min() vs shipping comparator.

mod exceptions;
mod floor_sum;
mod model;
mod query;

pub use exceptions::{FlatExceptions, RowBucketIndex};
pub use model::{decode_wire, regions_to_grid, ModelType, Region, WireError};
pub use query::{brute_sum, region_sum_rect, region_sum_tree, Rect};
