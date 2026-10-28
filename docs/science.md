# The Science of Manifold — Foundations of the Compression Methods (informative)

Status: **informative** — not part of any format specification. This explains
the information theory, coding theory, and statistical mechanics behind the
mechanisms used or evaluated in MNF5/MNF6/MNF7 and the MNF8 research track.
Every number quoted is measured in-repo; the study documents are the canonical
sources: `docs/benchmarks.md`, `docs/atlas-eval-v12.md`, `docs/subspace-v01.md`,
`docs/entropy-backends-v13.md`, `docs/adr-001-color-transform.md`. Bracketed
numbers refer to the reference list at the end.

## 0. One-line map

| Manifold mechanism | The idea | Foundation | In-repo evidence |
|---|---|---|---|
| Mod-256 residuals | (x−p)+p = x in Z/256 | ring bijection | every mode, §7 |
| MED/plane prediction | model residual distribution | conditional entropy [1],[11] | gradient 219,782→4,321 B [v12] |
| Tile atlas | domain-specific LZ dictionary | Ziv–Lempel universality [5],[6] | exact refs beat literals on noisy repeats |
| D8 geometric atlas | LZ matching on group orbits | dihedral symmetry | plaid +0.5%, ≈0 elsewhere [v12] |
| Huffman | optimal prefix coding | Kraft–McMillan [3] | deflate literals [4] |
| FSE/ANS (zstd) | arithmetic coding at Huffman speed | ANS inverse map [8],[9] | zstd-19/22 ≤ zlib-9 on residual streams [v13] |
| Range coding (xz) | −log₂p per symbol | arithmetic coding [7] | gradient literal −75% [v13] |
| Static dictionary (brotli) | free side information | side-information bound [10] | solid 512→22 B [v13] |
| YCoCg-R lifting | integer-bijective decorrelation | lifting factorization [13] | MNF6 mode 3 |
| Modular color (MNF7) | u8 wrapping lifting | bijective arithmetic | 0 corpus selections [ADR-001] |
| PCA subspace (research) | low-rank models | Eckart–Young [14], MDL [15] | FAIL gate, −131…−191% [v01] |
| rANS + residual model (MNF8) | coder sees encoder context | chain rule [2] | queued |

## 1. The counting argument — what compression can and cannot do

Of the 2^(8n) possible byte strings of length n, at most 2^(8n−k) can map to
outputs at least k bits shorter — so no lossless scheme shrinks even 1/256 of
all possible files by one byte without growing others. A compressor is a bet
on structure, and its inverse (the decoder) must know the same bet. Two
theorems sharpen this:

- **Source coding (Shannon [1]):** the expected length of any lossless code is
  bounded below by the entropy H(X) = −Σ pᵢ log₂ pᵢ (bits per symbol), and
  codes exist that approach it. For the entropy *rate* of a sequence source,
  the bound is H(X|x-history) — the conditional entropy of the next symbol
  given everything already coded.
- **Incomputability (Kolmogorov [16]):** no computable compressor is universal;
  every algorithm encodes a prior over inputs. Our measured consequence —
  "no backend dominates; the best backend is stream-dependent" — is exactly
  this theorem made empirical [v13, finding 1].

In-repo verification: the noise image is uniform iid pixels; its pixel payload
sits at exactly 8.000 bits/byte, the theoretical floor. Literal streams
compress to within ~0.03–0.13% of the raw stream across all backends
(196,935 B zlib-9, 196,869 B brotli) [v13]; nothing can do better, and the
residual overhead is framing a generic coder cannot see — the standing case
for context-informed coding (§5, §10).

## 2. Why images compress — correlation, prediction, entropy rate

A pixel is nearly determined by its neighbors (smooth surfaces, continuous
illumination), so H(pixel | context) ≪ H(pixel). That gap is the only thing a
lossless image codec harvests. Manifold splits the work the same way the
literature does [11]:

1. **A model reshapes the distribution** — prediction subtracts the
   deterministic part. MED (the JPEG-LS median-edge predictor: left, top, or
   the plane a+b−c where the local gradient is flat) is exactly LOCO-I [11].
2. **A coder compresses the remainder** at (approximately) its empirical
   entropy.

Measured at coder parity: the gradient image's literal stream is 219,782 B
under zlib-9; the MED-residual stream of the *same bytes* is 4,321 B — a 98%
cut with the **coder unchanged** [v12]. The corollary measured in [v13] is the
mirror image: hand smooth data straight to a match-finder coder and it *loses*
(zstd 230,715 B vs zlib-9's 219,782 B, every level including --ultra-22 — no
exact repeats exist for LZ77, so it emits store blocks), while a trivial
byte-delta (one-step prediction) hands the same coder 998 B. Models before
coders; coders never rescue an unmodeled source.

## 3. Dictionary coding — LZ and the sliding window

LZ77 codes a stream as literals plus (distance, length) pointers into a
sliding window of recent history; Ziv and Lempel proved such schemes are
asymptotically optimal — the ratio converges to the entropy rate for
stationary ergodic sources as the window grows [5],[6]. DEFLATE wraps LZ77
parses in Huffman-coded literal/length/distance trees [4]; zlib is the RFC
1950 container around DEFLATE.

MNF5's atlas is this idea lifted into the image domain: the "window" is the
last 128 prior tiles, the "alphabet" is RGB tiles, and a reference is a
(index, transform) pointer. The known honest weakness is the same one DEFLATE
has: matching starves without repeats — and on prediction residuals, where the
model already removed most structure, the metadata cost of a reference usually
isn't earned back [v12: atlas-on-residuals ≈ 0].

## 4. Entropy coders — Huffman, arithmetic, ANS

- **Prefix codes.** Kraft–McMillan [3] characterizes which codeword lengths
  form a valid prefix code; Huffman [2] is the optimal such code. Its ceiling:
  it pays integer bit lengths, wasting ~1 bit when a symbol's probability
  approaches 1 — which is why large-alphabet/finite-context coders exist.
- **Arithmetic coding** [7] encodes a symbol s in effectively −log₂ p(s) bits
  by refining an interval — entropy-rate-optimal per symbol under exact
  arithmetic; byte-friendly range-coder variants power LZMA/xz.
- **ANS** [8],[9] is arithmetic coding turned inside out: encode symbol d with
  frequency f_d over an alphabet total M = 2^R by the integer map
  x' = ⌊x/f_d⌋·M + (x mod f_d) + C_d. Because x' ≥ x the evolution is exactly
  invertible (state never loses information), and interleaving the state's low
  bits into the bitstream makes decode branch-free and fast. zstd's FSE is the
  table-driven tANS instance with 2^R states; zstd pairs it with Huffman-coded
  literals and LZ77 matching (RFC 8878 [18]).

Measured [v13]: at frozen-stream parity, zstd-19/22 beats zlib-9 on
mid-entropy residual streams (screenshot-med 3,007 → 962 B, −68%) but loses on
smooth literals (§2) — the coder family is an instrument, not a policy.

## 5. Context modeling and side information

The chain rule of information [2] says refining context never raises
conditional entropy: H(X | C₁) ≤ H(X | C₀) when C₁ makes more information
available to the model. All the strong late wins are richer context:

- **brotli** [17] couples LZ77 with per-context prefix coding and a ≈122 kB
  static dictionary shared by every decoder — decoder-local side information
  that costs the *stream* nothing. Measured free win: the solid-image literal
  stream 512 → 22 B [v13]. The cost is real and must stay in the accounting:
  the dictionary ships with the format (and our test binding cannot disable
  it) — recorded as a caveat, not a claim [v13].
- **LZMA/xz** feeds the previous bytes' bit-patterns into an adaptive range
  coder over a very large match window; it wins smooth streams (gradient
  literal −75%) but its decode-time memory/speed contract is wrong for a
  shipping image codec, so it is an archival comparison column only [v13].
- **MNF8's plan** is the direct application to Manifold's own state: an rANS
  coder whose context includes the predictor mode, the channel, and
  quantized neighboring reconstructions. A generic coder sees bytes; only our
  coder can know a residual came from MED's flat branch, or belongs to the Co
  plane after a rot-180 reference. Each context split is a measured
  −log₂p reduction that DEFLATE/FSE cannot spend. (The zero-run + Rice
  half of the study is the same statement for the highly peaked
  residual distributions MED leaves: Golomb–Rice codes are optimal for
  geometric distributions — the standard lossless-image coder family since
  JPEG-LS [11].)

## 6. Symmetry — the D4 group and the geometric atlas

The eight transformations of the dihedral group D4 (identity, flips, 180°,
transpose, and composites) are the exact symmetries of the square pixel
lattice. If patch B = T_g(A), then B carries A's statistics relabeled by g —
matching under the group is LZ matching on group *orbits*, costing one extra
symbol per use (the transform id). MNF5's mode 2 is precisely that.

The measured honesty: on top of a real predictor the D8 extension added ≈0
across the corpus; the sole true positive was plaid (+0.5%) — content with
genuine rotation symmetry [v12]. Exact pixel-level symmetry is rarer than
statistical similarity; the metadata byte must be earned per use. This is the
difference between "rotation invariance" as an assumption and as a measured,
gated mechanism.

## 7. Exactness — rings, lifting, and bijections

Losslessness in Manifold rests on small, verified algebra:

- **The ring Z/256.** Every residual is r = (x − p) mod 256 and every
  reconstruction is p + r mod 256 = x. Wrapping byte arithmetic is a
  bijection family on (Z/256)^3 — this is the entire losslessness guarantee
  of the atlas, independent of prediction quality (a bad prediction costs
  bytes, never truth).
- **Lifting.** The YCoCg-R transform [12] is a predict–update lifting chain
  with integer rounding; Daubechies and Sweldens proved finite wavelet-like
  chains factor into such steps, giving exactly reversible *integer*
  transforms [13]. The price is +1 bit of chroma dynamic range — the reason
  MNF6's mode 3 packs 9-bit planes rather than truncating (truncation destroys
  the bijection; rejected in the ADR).
- **Modular color (MNF7).** The ADR's alternative: perform the same lifting
  entirely in u8 wrapping arithmetic — a different bijection over RGB8
  triples, cheaper to serialize, discontinuous where the chain wraps.
  Selected 0 times on the corpus; kept because mechanism cost must be
  measured, not assumed [ADR-001].

## 8. Low-dimensional models — PCA, MDL, and the manifold hypothesis

PCA/SVD yields the variance-optimal low-rank model of a patch family
(Eckart–Young [14]). For *lossless* coding a basis is only a predictor, and
the true cost model is Rissanen's MDL: model description + coded residual
[15]. Measured: a serialized quantized basis (~6 kB) cannot amortize over
hundreds of 768-byte tiles, and fp-discovery → integer-synthesis noise
inflates residuals by 131–191% [v01]. The retention gate rejected the mode —
the honest outcome, and the reason "learned" methods here are research
tracks, not format changes.

The project's namesake is the broader physics-flavored hypothesis that
natural data concentrates near low-dimensional manifolds [19]; the tractable
first approximation is not a global basis but *local context in the entropy
coder* (§5) — which is why MNF8's rANS track precedes any resumption of the
subspace work.

## 9. The physics of the word "entropy"

Shannon's H has the same functional form as Gibbs' and Boltzmann's statistical
entropy, −k Σ p ln p; the two count the same thing (multiplicity of
microstates / messages) with different units [1],[20]. Shannon recounted that
von Neumann suggested the name for precisely that reason [21]. Contemporary
information theory treats coding as physics of the *typical set*: nearly all
probability mass sits in ~2^(nH) of 2^(nH(X+ε)) messages, and good codes
enumerate typical sequences — arithmetic coding's interval refinement is a
constructive walk on that set.

One boundary drawn honestly: **Landauer's principle** [22] prices *erasure* —
kT ln 2 ≈ 2.9×10⁻²¹ J per bit destroyed at room temperature. Lossless
compression erases nothing; it is a bijective re-encoding, computationally
reversible in principle. The thermodynamic floor therefore bounds lossy
discard, not this codec — the "physics" here is information geometry, not
heat.

## 10. What this buys MNF8

Every mechanism above decomposes into **model × coder**. The measured record
across the studies is consistent: models did the heavy lifting where they
applied (MED: −98% on smooth content), coders rank behind models at equal
inputs (backend deltas −75%..+5% on identical bytes [v13]), and metadata
overheads decide close cases (atlas ≈ 0 on residuals [v12]; MDL [15] says
this always: L = L_model + L_residual). MNF8's research bet is the coder that
consumes model state (§5) — the chain rule guarantees the gain is there;
the studies measure whether it survives metadata, decoder bounds, and the
retention gate. Formats change only after it does.

## 11. The measured research principles (2026-10-27 probe batch)

The 2026-10-27 probe program (six geometry-family denials + one engine
retention) sharpened §1–§3 into operational rules. Each is cited to its
measured record; the bracketed docs are canonical.

1. **Reordering is not compression.** A reversible permutation preserves
   the byte multiset; a scan that only *reorders* cannot beat a coder that
   already sees the same multiset. Measured: metric-guided traversal loses
   3.8×/17.5× on smooth/plaid, ties noise; Hilbert/Morton never beat
   raster+MED [riemannian-traversal-probe]. Any traversal claim must hence
   be a *prediction-context* claim, not an ordering claim.
2. **Geometry that preserves local angles or warps scale by less than the
   coder's noise floor is a no-op.** Conformal maps scale all neighbor
   distances by the same pointwise factor ⇒ causal-neighbor *rankings* are
   position-invariant; the atlas window spans ≤0.2% of the conformal scale
   variation, so d_f is a monotone warp of the Euclidean distance and a
   monotone warp cannot change which candidate ranks first except among
   cost-ties [mnf8-riemann-conformal-probe]. Measured: conformal vs plain
   Euclidean tiebreak differ by weight-tuning noise on every file.
3. **Topology is a 1/W-sized lever.** Reassigning prediction context only
   at the boundary (shear-wrap torus: x=0 takes the previous row's tail)
   touches 1/W of the plane; the mechanism's ceiling is ~±1% on any image,
   and its *direction* depends on whether row-heads continue row-tails
   (photos: no — +0.0…+0.11% measured loss; constructed diagonal textures:
   −0.6…−1.5%, the best case) [riemann-array-v05-probe]. 0/30 corpus wins.
4. **A transform earns its description or it does not exist.** MDL [15]
   applied per mechanism: a geometric reference pays (idx, transform-id)
   bytes and wins only where the transformed prediction beats the residual
   cost. Matrix composition (T = T₃T₂T₁) can only pay when the composed
   description is shorter than one transform id — measured: composed and
   dictionary-compressed chart references never fire on photos, ≤1% on two
   synthetics, and the shipping D8 chart grammar already provides the wins
   [mnf9-engine-ladder-probe]. The matrix algebra is a compact *notation*
   for transforms the codec already charges honestly.
5. **The one measured engine win: repetition machinery at coder parity.**
   LZ77-2D tokens over the MED residual plane (tag+payload, window 64k px)
   is Ziv–Lempel [5],[6] applied to the *token* plane: gradient −74%
   (1,639→419 B), solid −34% (beats WebP-lossless), blocks −6% — while
   forced-everywhere it costs 1.151× MNF8 overall because noise-dominated
   residual planes pay for accidental matches. The retained form is the
   rate-aware per-file pick (never worse than the picked literal arm,
   +1 flag bit) — the same gate the D8 atlas lived behind [v12].
6. **The honest-results contract extends to geometry claims.** Six of
   seven proposed geometric mechanisms measured dead under this gate
   (traversal, metric-guided order, metric-warp predictor, D8+shear refs,
   conformal guidance, shear-wrap torus); the retained list is exactly one
   item (the token engine) plus one recorded single-class candidate
   (gradient-orientation tiebreak, document-class −5.5%
   [mnf8-riemann-conformal-probe]). The denials are part of the science
   record, not footnotes: each one is where a plausible-sounding theorem
   met the counting argument.

## References

[1] C. E. Shannon, "A Mathematical Theory of Communication," *Bell System
Technical Journal* 27(3,4), 1948, pp. 379–423, 623–656.

[2] T. M. Cover, J. A. Thomas, *Elements of Information Theory*, 2nd ed.,
Wiley, 2006 (source coding theorem; conditional-entropy chain rule; typical
sets).

[3] L. G. Kraft (1949); B. McMillan (1956) — the Kraft–McMillan inequality
for uniquely decodable / prefix codes. Standard treatment in [2], ch. 5.

[4] P. Deutsch, "DEFLATE Compressed Data Format Specification," RFC 1951,
1996; P. Deutsch, J.-L. Gailly, "ZLIB Compressed Data Format Specification,"
RFC 1950, 1996.

[5] J. Ziv, A. Lempel, "A Universal Algorithm for Sequential Data
Compression," *IEEE Trans. Information Theory* IT-23(3), 1977, pp. 337–343.

[6] J. Ziv, A. Lempel, "Compression of Individual Sequences via Variable-Rate
Coding," *IEEE Trans. Information Theory* IT-24(5), 1978, pp. 530–536.

[7] J. Rissanen, G. G. Langdon, "Arithmetic Coding," *IBM Journal of Research
and Development* 23(2), 1979, pp. 149–162.

[8] J. Duda, "Asymmetric Numeral Systems," arXiv:0908.2941 (2009; later
"…as close to the Shannon limit as one could get," 2014 revision).

[9] J. Duda, R. Tahboub, N. K. Gadgil, E. J. Deller, "The Use of Asymmetric
Numeral Systems as an Accurate Replacement for Huffman Coding," *PCS —
Picture Coding Symposium*, 2015.

[10] A. El Gamal, Y.-H. Kim, *Network Information Theory* (side-information
bounds, Slepian–Wolf), Cambridge Univ. Press, 2011 — general background for
dictionary-as-side-information.

[11] M. J. Weinberger, G. Seroussi, G. Sapiro, "The LOCO-I Lossless
Compression Algorithm: Principles and Standardization into JPEG-LS," *IEEE
Trans. Image Processing* 9(8), 2000, pp. 1301–1314. Standardized as ITU-T
T.87 / ISO/IEC 14495-1.

[12] H. S. Malvar, G. J. Sullivan, "YCoCg-R: A Color Array Transform with
Lifting," Microsoft Research technical report, 2003 (basis of the lossless
YCoCg-R variant in modern video coding).

[13] I. Daubechies, W. Sweldens, "Factoring Wavelet Transforms into Lifting
Steps," *J. Fourier Analysis and Applications* 4(3), 1998, pp. 247–269.

[14] C. Eckart, G. Young, "The Approximation of One Matrix by Another of
Lower Rank," *Psychometrika* 1(3), 1936, pp. 211–218.

[15] J. Rissanen, "Modeling by Shortest Data Description," *Automatica*
14(5), 1978, pp. 465–471 (minimum description length).

[16] M. Li, P. Vitányi, *An Introduction to Kolmogorov Complexity and Its
Applications*, Springer (incomputability; distributions on strings).

[17] J. Alakuijala, Z. Szabadka, "Brotli Compressed Data Format," RFC 7932,
2016 (context modeling; ≈122 kB static dictionary).

[18] M. Kucherawy (ed.), "Zstandard Compression and the 'application/zstd'
Media Type," RFC 8878, 2023 (FSE/tANS description; window; levels).

[19] C. Fefferman, S. Mitter, H. Narayanan, "Testing the Manifold Hypothesis,"
*NeurIPS*, 2015.

[20] J. W. Gibbs, *Elementary Principles in Statistical Mechanics*, 1902
(the −Σp ln p form).

[21] M. Tribus, E. C. McIrvine, "Energy and Information," *Scientific
American* 225(3), 1971, pp. 179–188 (Shannon's account of the entropy
naming).

[22] R. Landauer, "Irreversibility and Heat Generation in the Computing
Process," *IBM Journal of Research and Development* 5(3), 1961, pp. 183–191.

[23] constriction — Rust entropy-coding library (range coding, rANS,
interleaved rANS): docs.rs/constriction (candidate for the MNF8 residual
coder research).

---

*Reproducibility note:* measured numbers cite the study that produced them;
backend matrix — `crates/mnf-subspace/scripts/entropy_backends.py`; predictor
ablation — `crates/mnf-subspace/scripts/ablate_abcd.py` and
`cargo test -p mnf-subspace --release`; shipped-codec claims —
`cargo test` (conformance, differential, mnf6_conformance suites).