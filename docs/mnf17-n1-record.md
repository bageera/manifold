# MNF17-N1 — native production entropy-stream acquisition (2026-11-18)

Native-replay gate under the restored methodology (Python = screening,
Rust = confirmation). This instrument extracts the ACTUAL symbol streams
Manifold's production candidate matrix hands to the entropy backends —
with genuine decoder-available geometry — so the BZ16 Boltzmann weighting
can be replayed against them (N2) under complete wire accounting (N3).
Production code is untouched: the acquisition lives in an env-gated
`#[ignore]` instrument (`mnf17_n1_acquisition` in `src/mnf8.rs`).

## Instrument semantics

Per cohort fixture the instrument enumerates the production candidate
matrix exactly as the pick builds it (record arms: predictor classes
{med,left,none,plane,interleaved,bilinear}, planar ×{off,on}, the coder
classes {mix4,mix5,mix6,mix7,zstd19,zlib}, references 0–3; token arms:
token16/tokenmax), encodes each arm through all non-MIX7 backends, and
reconciles the winner against a freshly computed production pick by
**exactly one distinct stream** (assembled-file byte identity over
backends; arms sharing one token wire collapse only on exact raw-byte
equality — fnv is used for indexing in the ledger, never as proof of
identity). Winner and closest-declined arm per file are emitted.

The emitted binary per arm (`research/results/mnf17-n1/*.bin`,
format `N1G2`) is a per-byte record:

    byte, class (LaneWalk class sequence), lane (0|1|2|255),
    causal left residual, causal upper residual, u32 global pixel index,
    … ineligible tail (N1IN rows only, no geometry), u32 eligible_upto.

Geometry comes from a `PlaneWalk` that mirrors the production `LaneWalk`
state machine (`consume` + record-shape table copy) and asserts per byte
that its class sequence equals the real `LaneWalk`'s — the class parity
required by the gate, over all 14 emitted streams. Left/upper residuals
are read from the lane planes at the byte's own (row, col) **before** the
byte is written, 0-padded at plane edges: decoder-available by
construction, since the decoder reconstructs those exact planes byte-wise
while consuming the same stream (`LaneWalk::consume` fills
`lanes[lane][row·tw+col]` per body byte). Model-reset boundaries are
preserved as class-0 boundary bytes the walker resets at; the payload
sizes of all six backends per arm are in the summary ledger, which holds
the original candidate ranks and aliases (arm_idx) untouched by
deduplication.

Token arms are acquired for identity/payload accounting but are
**ineligible for geometry** (their per-tag wire layout needs its own
walker; recorded, not silent).

## Accepted dataset

Winner arms (payload = the winning backend's compressed size, per the
assembled-file identity check; full six-backend payload rows per arm in
the ledger):

| fixture | winner arm (kind/planar/coder/ref) | backend | payload (B) | raw_len | eligible_upto |
| --- | --- | --- | --- | --- | --- |
| solid_256x256 | med/false/zstd19/ref0 | mix6 | 30 | 196,864 | 196,864 |
| gradient_320x240 | left/false/zstd19/ref0 | mix6 | 222 | 230,700 | 230,700 |
| repeat_256x256 | left/true/zstd19/ref0 | mix6 | 269 | 196,864 | 196,864 |
| screenshot_640x400 | plane/false/zlib/ref3 | mix7 | 440 | 769,000 | 769,000 |
| noise_256x256 | none/false/zstd19/ref0 | mix6 | 196,880 | 196,864 | 196,864 |
| forest s900002 | left/true/zstd19/ref1 | mix7 | 40,660 | 107,660 | 107,660 |
| terrain s3100000 | plane/false/zstd19/ref0 | mix7 | 13,204 | 57,680 | 57,680 |

(noise's 196,880 vs 196,864: the payload adds the 16-byte framing to the
196,864-byte raw stream — the stream passes through essentially
uncompressed, as expected for the negative control. gradient's and
noise's picks are byte-identical under zstd19/zlib arms — the same wire
carries both labels; both stay in the ledger, one distinct stream each.)

All seven **winner streams are fully geometry-eligible** (`eligible_upto
== raw_len`); the seven closest-declined token16 arms are ineligible by
construction (their own tail additionally carries no record-grammar
geometry — marked in the ledger, e.g. solid's token16 tail from byte
2,311; gradient's at 769, etc.). Hubble follows the screening cohort.

Full per-arm ledger: `research/results/mnf17-n1-summary.csv`
(18 columns: file, arm_idx, kind, planar, coder, reference, stream_fnv,
raw_len, selected_backend, emitted(winner/closest_declined), eligible,
eligible_upto, and the six per-backend payload sizes). Binaries:
`research/results/mnf17-n1/` (14 files, ~17 MB).

## Gate status (operator acceptance table)

| Requirement | Disposition |
| --- | --- |
| Instrument compilation | PASS (release build; run RC=0) |
| LaneWalk class parity | PASS — per-byte assert over all 14 streams |
| Genuine causal geometry | PASS — plane reads at (row, col) pre-write, decoder-reconstructible; left/upper availability structural |
| Winner-wire reconciliation | PASS — exactly one distinct stream per file, byte-exact (incl. the gradient token-wire case) |
| Equivalent-stream reconciliation | PASS — exact byte comparison before collapse; fnv indexed only |
| Token-arm acquisition | PASS — identity/payload rows present, geometry-ineligible by construction |
| Ineligible geometry accounting | PASS — per-arm eligible/eligible_upto in the ledger; winner arms all fully eligible |
| Seven-file dataset | PASS |
| Research record | this document |
| Both repository lineages | sync at commit |

## Next (CONDITIONAL per authorization)

N2 six-arm replay (A0/A1/B0/B1/C0/C1) on these streams,
ΔL_energy = L_B1 − L_B0, ΔL_correction = L_C0 − L_A1, ΔL_mixture =
L_C1 − L_C0 — primary Q12, precision controls Q8/Q16, the symmetric clamp
retained as a labeled ablation; partially-eligible streams stay excluded
from geometry-dependent arms. Then N3's per-component cost harness and
complete-wire ΔL_wire vs A0 and the production portfolio.
