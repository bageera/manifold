# MNF16-X1-L R4 record (2026-11-15) — independent self-contained decoding gate

L1 frozen exactly as measured (docs/mnf16-x1l-record.md: corpus
+41,720.9 B model / +41,708 B wire; Hubble +12,652 B wire). R4 removes
the last external input from the experimental representation: the lane
class must be regenerated natively, from decoder-accessible state, when
each probability is evaluated.

## Implementation (additive instrument tokens; production untouched)

`nat` mode token on the r9_audit instrument + `walk_classes`:
a bounds-checked record-grammar walk that derives the per-byte lane
class from (a) the container metadata riding in the audit header
(w, h, tile_count, flags — fields after n) and (b) the stream's own
framing bytes (mode byte + the committed META table + body cursor).
Per byte: framing bytes class 0 (structural, never residual operands);
interleaved bodies class 0,1,1 repeating; planar bodies lane-major
(tw·th zeros, then tw·th·2 ones). Malformed framing returns None ->
clean exit 1 (no panics): bounds-checked indexing and a
consumed-everything structural check (`off == stream.len()`).

The class is a pure function of the decoded prefix + header metadata:
T0–T4 stand by the X1-0 proof (the walk consumes mode+meta before each
byte; positions are strictly prefix-determined), and the parity check
below verifies the native regeneration byte-for-byte against the
external-array path that the census validated.

## Gate results

| Gate | Result |
| --- | --- |
| Quantized probability parity | PASS — native `a` words == external-array `a` words EXACTLY, all 22 corpus files (word-for-word f64 equality incl. roundtrip state) and Hubble (2,628,054 bytes, flags 0x81) |
| Exact reconstruction | PASS — the audit's exact-inverse contract (fresh decode-side models evolved from decoded symbols; decode(encode(x)) == x) asserted on every native run |
| Self-contained wire | PASS — the native wire carries NO class bytes; wire bytes identical to the recorded external values on every file (forest 40,728 = the recorded wire1) |
| Complete accounting | PASS — entropy-wire bytes include all framing (stream-grid coding); experimental wire = 541,732 B corpus-class total unchanged from the record |
| Native costs | walk overhead ratio 1.01x on the audit path (forest native audit 0.247 s vs external 0.245 s); the python 365 s walk (2.63M bytes) reduces to a linear Rust pass (effectively free relative to the rANS roundtrip); instrumentation only |
| Robustness | PASS — 60-case fuzz on the native path (truncations at random points, random byte flips including mode/meta, wrong tile counts, tail cuts): 60/60 clean outcomes (15 structurally-valid mutations roundtrip with rc 0; the rest exit cleanly), no hangs, no crashes |

Sweep: 22/22 corpus files parity + wire-equal
(/tmp/r4_sweep.log); Hubble parity on the first planar probe (0x81).

## Disposition

R4 PASSES. The experimental wire for L1 is now fully self-contained:
container metadata + entropy stream reconstruct the symbols AND the
conditioning state, with exact reconstruction and byte-exact accounting.
Per the promotion contract, R5 (complete Maximum portfolio accounting on
the production encoder path with independent decode) is the remaining
gate before any promotion decision; NOT executed here. Still no
production backend identifier. Anchor 810,816 B untouched; the audit
extension remains additive (modes without `lanechild`/`nat` untouched).
