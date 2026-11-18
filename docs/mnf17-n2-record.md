# MNF17-N2/N3 — native probability replay + cost harness (2026-11-18)

The six authorized arms replayed on the MNF17-N1 acquisition binaries
(all seven winner streams, fully geometry-eligible), per the operator
memo: ΔL_energy = L_B1 − L_B0, ΔL_correction = L_C0 − L_A1,
ΔL_mixture = L_C1 − L_C0; Q12 primary, Q8/Q16 precision controls, the
symmetric ±c_lim clamp run as the labeled Q12 ablation. N3 adds the
per-component compute timings and the incremental 18-bit CACM87
serialization per arm (complete serialized costs). Instrument:
`src/mix_coder.rs::mnf17_n2_replay` (env-gated `#[ignore]`, research
instrument; production behavior untouched). Engine mirrors
`crates/mnf-mans/examples/bz16_r2h_parity.rs` verbatim (frozen LUT fnv
`0x50abef206188b374` asserted; asymmetric [−S, 4S] clamp, S = 2^Q).

## Arm definitions (exact)

| Arm | Model |
| --- | --- |
| A0 | native production: each stream's actual payload at the winning backend (ledger) |
| A1 | production entropy-model family replayed per symbol: backend-7 lane-class mix (params frozen `MixParams::BACKEND7`), ideal cost −Σlog2(p/2^24) |
| B0 | conventional BZ16: the recovered engine verbatim (adaptive 4-expert mixture: hi/lo Boltzmann, counter unigram, uniform; fixed-share α=0.999; features (v,0),(v,up),(v,left) from the N1 geometry; context-only (0,0) features for mode/meta bytes) |
| B1 | standalone Boltzmann: the hi expert alone, normalized from its LUT row |
| C0 | production prior × Boltzmann tilt: p_mix ⊙ p_hi, per-position renormalized |
| C1 | production-conditioned expert mixture: recovered fixed-share on two slots {p_mix_q15, p_hi}, recovered constants (×999, +16384, /1000, Σw = 2^16) |

Serialization-domain choices (all deterministic, disclosed): B1/C0/C1
consume the hi expert at its **Q24 LUT resolution** rebalanced through
largest-remainder floors (the Q15 window keeps only ~11 unsaturated
steps; every other model output can floor to all-zero — the recovered
coder's own zero-mixture fallback covers B0's Σm = 0 case, triggered on
gradient-Q16). The mix-family ideal cost (A1) validated against the real
backend payloads: solid 352 bits≈44 B (payload 52), gradient 51,940≈6,493 B
(6,504), screenshot 3,419≈427 B (440), terrain 105,570≈13,196 B (13,464)
— agreement ≤ ~0.3% on the natural streams.

## Results — Q12 primary (ideal bits on the full symbol sequence)

| fixture | A1 | B0 | B1 | C0 | C1 | ΔL_energy (B1−B0) | ΔL_correction (C0−A1) | ΔL_mixture (C1−C0) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| solid | 352 | 865,965 | 1,055,452 | 2,288 | 2,412 | +189,487 | +1,936 | +124 |
| gradient | 51,940 | 183,542 | 413,026 | 71,329 | 179,305 | +229,484 | +19,389 | +107,976 |
| repeat | 206,254 | 724,425 | 1,114,377 | 204,266 | 207,845 | +389,952 | **−1,988** | +3,579 |
| screenshot | 3,419 | 246,652 | 1,144,352 | 13,149 | 306,493 | +897,700 | +9,730 | +293,344 |
| noise | 1,600,729 | 1,575,034 | 2,064,739 | 1,888,333 | 1,628,643 | +489,705 | +287,604 | **−259,690** |
| forest | 325,207 | 349,514 | 534,835 | 463,961 | 328,253 | +185,321 | +138,754 | **−135,708** |
| terrain | 105,570 | 121,544 | 182,747 | 134,158 | 124,974 | +61,203 | +28,588 | **−9,184** |

Wire bytes (18-bit CACM87, R2H pins) track the ideal costs (e.g.
terrain C1 = 124,974 bits → 124,974 B in the CSV); production A0 =
30/222/269/196,880/440/40,660/13,204 B. Q8/Q16 orderings are stable, and
the **Q12 symmetric-clamp ablation is cost-identical on all seven**
(the clamp never binds at Q12 on these streams — the R2H-E finding
re-confirmed on native data).

## Findings

1. **ΔL_energy > 0 on all seven at every precision** (B1−B0 =
   +61,203 … +897,700): the standalone Boltzmann model alone is always
   worse than the recovered mixture — the adaptive weighting machinery
   (counter, uniform, fixed-share over experts) is necessary to the BZ16
   candidate. Mixing helps; it is not sufficient.
2. **ΔL_correction > 0 on 6/7** at Q12: multiplying the production prior
   by the Boltzmann tilt loses bits everywhere except `repeat`
   (−1,988 bits, −0.97%; also negative at Q16, −4,933). The correction
   does not survive on the production-conditioned path.
3. **ΔL_mixture < 0 on 3/7** (noise −259,690; forest −135,708; terrain
   −9,184): the adaptive two-expert mixture beats the fixed product —
   but only on photographic/terrain streams, and the margin is far from
   closing the gap to A1.
4. **Complete serialized costs against A0 and the portfolio**: no arm
   approaches the production pick on any stream (best BZ16 wire vs A0:
   solid 2,286 vs 30 B; gradient 71,331 vs 222 B (closest is C0 at
   8.9 KB vs A0 222 B, but note the production pick here is ZSTD19 —
   generic zstd beat the entire Manifold mix family on gradient); noise
   1,612,290 vs 196,880 B; terrain 124,974 vs 13,204 B).

**Disposition: the central BZ16 question resolves NEGATIVELY at N2 on
the native cohort** — Boltzmann weighting over the three causal features
captures nothing the production entropy model misses on these streams,
and every serialized BZ16 arm remains far above A0. The per-arm ΔL
structure is exactly what the six-arm isolation was designed to expose:
the mixture machinery is essential (energy), the tilt is harmful
(correction), and adaptive mixing between production and Boltzmann
components yields real but small gains over a fixed product
(ΔL_mixture < 0 on natural streams).

## N3 — per-component compute (median-ish across cohorts)

| Component | ns/position |
| --- | --- |
| Production mix step (backend-7 replay: walk + context key + 2^24 probs + adaptive update) | ~520–800 |
| BZ16 engine step (256-symbol mixture + LUT ×2 + counters LR + adapt + 4 incremental coders) | ~27,000–67,000 |

The BZ16 model layer costs ~50–100× the production mix step per
position in this replay form — a production-integration blocker on cost
grounds regardless of ΔL (the dominant term is the per-position 256-slot
renormalization chain; a slot-incremental redesign is the documented
upgrade path and unattempted, per the HOLD ledger).

## Acceptance/ledger

| Item | Disposition |
| --- | --- |
| Six-arm replay on the N1 cohort | PASS (28/28 rigs, RC=0) |
| ΔL_energy / ΔL_correction / ΔL_mixture | recorded (tables above) |
| Q8/Q16 controls | recorded, ordering stable |
| Symmetric-clamp ablation (Q12) | cost-identical (clamp never binds) |
| Complete serialized costs vs A0 + portfolio | recorded, NEGATIVE vs A0 |
| Per-component compute + wire economics | recorded (~50–100× mix step) |
| Record + both lineages | this document + commit |

CSV: `research/results/mnf17-n2-summary.csv` (28 rows × 17 cols);
run log `research/results/mnf17-n2.log` (N2_RC=0). Hubble remains gated
behind the initial screening cohort and awaits its acquisition run;
production integration stays HOLD.
