# MNF17-P1-D0 — writer mathematical audit (the certified lower bound)

**Status**: D0 (source audit + mathematical statement only) executed
against the pinned writer implementation; all D1 census runs assert
every clause marked [D1-verified] post hoc.

## 1. The pinned implementation (constriction 0.5.0, stream/queue.rs)

`DefaultRangeEncoder = RangeEncoder<u32, u64, Vec<u32>>`: three parts —

1. `bulk: Vec<u32>` — the words written so far (append-only backend).
2. `state: RangeCoderState<lower, range>` — the arithmetic-coder
   registers (u64 state, 2^32-wide words).
3. `situation: EncoderSituation<u32>` — `Normal`, or
   `Inverted(num_subsequent, first_word)`.

Per the pinned source:

- **Normal**: "all full `Words` of compressed data have been written to
  the backend, and the internal coder state holds less than one word of
  additional information content."
- **Inverted**: "some full words of compressed data have been held back
  and not yet written to the backend ... because their final values may
  still change depending on subsequently encoded symbols. More
  precisely, ... the held-back words can become either `first_word + 1`
  followed by `num_subsequent` zero words, or `first_word` followed by
  `num_subsequent` words that have all bits set."

So word **values** in the inversion window are provisional, but two
properties are exact at all times:

- (P1) **Word count is exact:** `Pos for RangeEncoder` returns
  `bulk.pos() + num_inverted` — the total word count including words
  held back, never provisional.
- (P2) **Inversion resolution only rewrites values within the held-back
  suffix** (never the count, and never words already on `bulk`): both
  variants have identical word counts, one variant appends all-bit-set
  words, the other `first_word + 1` + zero words — same length.

## 2. Finalization (`seal`/`into_compressed`)

`seal()` appends `iter_seal(state, situation)`: the held-back words
(their final variant) plus **1 or 2 further words** identifying the
final range (2 iff `upper_word == point_word` per `num_seal_words()`;
0 words only for the completely empty coder, `range == State::max`).

`num_words()` = `bulk.remaining() + num_seal_words()` — the pinned doc:
"This method returns the length of the slice, the `Vec<Word>`, ... that
would be returned by get_compressed/into_compressed/iter_compressed,
respectively, when called at this time."

**This is the exact F_min + committed-word quantity** — the naive
`bulk.len()` would UNDERCOUNT by the held-back words + the 0-2 seal
words; `num_words()` includes both by construction. No separate F_min
term is needed or permitted.

[D1-verified] For every D1 mix arm the final series entry satisfied
`4·series[N] + 4 == payload.len()` (exact final accounting), and every
series was non-decreasing (monotonicity of the word count under symbol
append: bulk only grows; inversion resolution rewrites values, never
counts; seal adds ≥ 0).

## 3. The certified bound

For a mix-coded candidate m evaluated under container
`assemble_mnf8` (constant `HEADER8 = 32` byte prefix — magic, width,
height, TILE_SIZE, tile_count, payload_len, flags, backend fields,
verified [D1-verified] by `size − payload.len() == 32` per arm):

payload bytes = 4 (u32-LE stream-length prefix) + 4·(word count), so at
any encoder state t (after t symbols of arm m):

    B_m(t) = 32 + 4 + 4·num_words(t)

**Theorem (bound validity).** For every reachable state t,
`B_m(t) ≤ L_final(m) = 32 + 4 + 4·num_words(T)`, T = N_m.
Proof: `num_words(T) ≥ num_words(t)` by §2's monotonicity (P1, P2 and
append-only bulk). ∎

**Theorem (elimination safety with the production tie rule).** The
production selector keeps the first arm minimizing the tuple
`(size, rank)` over the ORIGINAL enumeration ranks — `better =
(size, rank) < (best_size, best_rank)` (mnf8.rs encode_mnf8_metrics);
incumbent size is non-increasing over the arm order. If at any t

    B_m(t) > incumbent_size(now)

then `L_final(m) ≥ B_m(t) > incumbent_size(now) ≥ incumbent_final_size`,
so m's final size is STRICTLY larger than the eventual best — m cannot
win, not even by the rank tie-break (ties require equal sizes).
Elimination on EQUALITY is not sound: the final best could still tie m
at `==` and m could win by rank. Hence the certified rule is strictly

    B_m(t) > L_incumbent  ⇒  skip all remaining work of arm m

never `≥`. This matches the conservative pruning rule in the D1
authorization. ∎

## 4. Termination side-effects audit

- **Shared state**: the selector is stateless across arms; cancelling
  an arm mid-encode discards only arm-local `RangeEncoder`/`MixCtx`
  structures. The D1 probe-cache is keyed by input identity, not by arm
  progress — aborting an arm cannot poison other arms.
- **Aborted-arm residue**: a declined arm never produces a container;
  decoding is unaffected (bound-based termination is encode-only; the
  decoder path is untouched by D0/D1/D2 semantics).
- **The probe does not perturb the model**: `num_words()` is a pure
  length read; C4-A context evolution is unaffected. D1 measures the
  probe's own cost for every interval (per-symbol `num_words` sampling
  cost recorded per arm; interval-k overhead ≈ that cost / k).

## 5. Deterministic tie-breaking under reordering (D1-B/C/Oracle)

Because the winner is min over `(size, ORIGINAL rank)`, evaluating arms
in a different order cannot change the winner — provided (a) every arm
still runs to enough of its evaluation to produce its true `size`, or
(b) is eliminated ONLY under §3's strict rule (then its size is known
to be strictly worse, so it cannot win regardless of rank). For the D1
census no arm is ever actually eliminated; arms run to completion and
crossings are recorded counterfactually, so every strategy observes the
same true arm sizes and the winner is identical across strategies.

## 6. Determinations (the D0 dependency table)

| Writer property        | Determination                                                              |
| --- | --- |
| Word emission          | Count exact at all times; `pos() = bulk + num_inverted` (P1)               |
| Pending carry          | Bounded inversion window; values provisional, count not (P2)               |
| Renormalization        | Internal to the coder; no byte-count effect                                 |
| Buffered words         | Held-back words are included in `num_words()` — usable                      |
| Finalization           | 0/1/2 words, already inside `num_words()`; no extra F_min term              |
| Word-to-byte           | 4 LE bytes/word + 4-byte u32 stream-length prefix, exact                    |
| Container framing      | `HEADER8 = 32` constant per arm [D1-verified]                                |
| Termination            | Stateless per arm; strict-inequality rule only (§3)                         |

## 7. Consequence for D1

The census measures `τ_m = min{t : B_m(t) > incumbent(t)}` for four
incumbent strategies (A production order; B zstd/zlib-complete-first;
C progressive cost-ascending incumbents; Oracle = final best revealed
at t=0) from the SAME per-arm `num_words` series, at intervals
1/16/64/256. All arms complete; symbols avoided are reported both
symbol-weighted and time-weighted (per-arm mean per-symbol cost).
