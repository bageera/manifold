# MNF17-P1-C5–C7 — C4-A production integration validation

**Status**: all gates executed on the integrated candidate. The only
production change is C4-A's maintained counter sums inside `MixCtx`
(two `u64` fields; +16 B/context). The pre-C4-A implementation is
retained verbatim as a `cfg(test)` differential oracle
(`mod oracle` in src/mix_coder.rs: `mix_encode_with_o`,
`mix_encode_with_lane_o`, `mix_decode_with_o`, `mix_decode_with_lane_o`).

## C5 — candidate replay + numerical state

- Full P1-B replay: every arm from the 1,216-arm decomposition cohort
  re-encoded through the integrated production encoder against the
  recorded v1.17.0 wires: **1216/1216 byte-identical (sha16 + length),
  0 divergent arms** (`/tmp/mnf17_p1b_c5.csv` run vs the recorded
  `/tmp/mnf17_p1b_rows.csv`).
- Numerical state: the oracle differential test
  (`c5_oracle_differential`) drives the §4 fixture set and a 20k
  random stream through both engines with per-symbol checks — quantized
  256-vectors, 512 counters, EWA `log_w` bits, `nlocal` — plus whole
  wire parity (plain, lane, decode bytes). PASS.
- Release build of the shipped conformance suite with the integrated
  encoder: all tests pass (see C7 gate below); forest payload hash
  re-verified `682e23165304b050` after integration.

## C6 — contemporaneous performance (192 arms, medians of 3)

Measured prod-vs-oracle inside the SAME release binary
(`/usr/bin/time` process: peak footprint 31,507,056 B ≈ 30 MB, no
regression vs the pre-integration profile; the maintained sums add
16 B/context = 640 B per evaluation):

| Path | Integrated C4-A | Oracle (pre-C4-A) | Speedup | Saved |
| --- | --- | --- | --- | --- |
| encode | 49.91 s | 57.45 s | **1.151×** | 13.1% |
| decode | 51.04 s | 58.25 s | **1.141×** | 12.4% |

Per-backend consistency (encode/decode): mix4 1.147/1.140×,
mix5 1.149/1.141×, mix6 1.159/1.148×, mix7 1.148/1.136×.
Contemporaneous encode speedup (1.151×) matches the pre-integration
factorial measurement (1.155×). **The decoder benefits essentially
equally** — the decode loop's per-symbol model rebuild consumes the
same maintained quantities.

## C7 — portfolio + held-out wire parity (release, integrated encoder)

- Disabled portfolio (legacy-only selection): **810,816 B** — exact.
- Enabled portfolio (frozen v1.17.0 selections): **768,303 B** — exact.
- Hubble 1000×872: **1,438,848 B** — exact; payload digest
  `b2a8a10915239c99`; reconstruction pixel sha `06ec02b845343304`.
- Wire parity: **60/60** portfolio members byte-identical + Hubble.
- Held-out fixtures (plaid_192, dusk_160x120_s900003 and the full
  conformance suite's negative cases): **0 wire differences, 0 decode
  divergences**.
- Release gate: `make gate` — fmt, clippy (0 warnings), tests, docs.

## Dispositions

| Gate | Result |
| --- | --- |
| C5 — replay + state | PASS · 1216/1216 byte-identical |
| C6 — performance | PASS · 1.151× enc / 1.141× dec, RSS +640 B |
| C7 — corpus | PASS · 810,816 / 768,303 / 1,438,848 exact, 60/60 |
| C7 — held-out | PASS · zero wire differences |

Every acceptance box in the authorization ledger is satisfied on the
integrated candidate. The shipped wire remains the frozen v1.17.0
wire — the integration is computationally identical (conserved
invariants replace recomputation), not semantically different.
