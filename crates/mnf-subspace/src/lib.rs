// MNF Subspace v0.1 — Exact Low-Rank Predictive Atlas (research).
//
// Three layers, strictly separated (ADR-style):
// - discovery: floating-point SVD/rank search over training patches or
//   prediction residuals (encoding-side only; never in the decoder)
// - basis: quantized integer bases B (i16 fixed point, scale s) shared
//   across a patch family + integer mean mu
// - predictor: deterministic integer synthesis xhat = clip(mu + (Bq)>>s),
//   residual r = (x - xhat) mod 256 => exact by construction
//
// The decoder needs NO floating point and NO SVD: it reads quantized
// basis + coordinates + residual. Discovery only decides what to store.

pub mod basis;
pub mod discovery;
pub mod experiment;
pub mod predictor;
