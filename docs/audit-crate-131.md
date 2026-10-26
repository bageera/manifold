# Published-Crate Audit — manifold-codec v1.3.1 (2026-10-26)

Scope: the artifact on crates.io (`target/package/manifold-codec-1.3.1`,
`manifold-codec-capi-1.3.1`), not the worktree. Method: package from the
v1.3.1 tree, then test/break the packaged tree and a fresh consumer project
that installs the registry crate.

## Pillar 1 — Mathematical correctness: PASS (measured on the artifact)

- Full test suite inside the PACKAGED tree (deps = registry, no path deps):
  53 tests / 7 suites, all pass — golden vectors decode exactly, rejection
  vectors reject without panics, differential (refdecode vs codec) exact.
- Fresh consumer (`cargo add manifold-codec@1.3.1`): MNF5 public pipeline
  (split_tiles → encode_stats → compress_payload → build_header) + decode
  exact; refdecode == codec; MNF7/MNF8 both profiles exact; malformed input
  → `Err(Error::Format)`, never a panic. Limit rejections match
  `Error::Limits`. (consumer project: /tmp/consumer-131)

## Pillar 2 — Scientific credibility: PARTIAL

- Internal reproducibility: packaged suites pass; benchmark numbers
  reproducible from the same tree (spot check: screenshot 2118 B PNG →
  782 B MNF8; honest loss preserved in docs/benchmarks.md).
- vs WebP-lossless/JXL: NOT yet reproducible by an outside user without
  installers (cjxl, cwebp) + scripts/bench.py. The honest-loss record
  exists (docs/benchmarks.md), but the external-comparison arm is not a
  one-command repro. (dusk held-out photo: MNF8 1.04x PNG — the loss class
  is real and already documented.)

## Pillar 3 — Developer usability: PARTIAL

- Docs: rustdoc complete (0 missing-doc; docs.rs serves all pages).
- Examples: 1 runnable (roundtrip).
- Discoverability findings for v1.4:
  1. Package hygiene: 1.4 MiB tarball. testdata/ (~460 KiB photos),
     scripts/, .github/.gitlab CI configs, docs/ all ship. No
     `[package] exclude`.
  2. [[bin]] + 4 [[test]] targets ship in the library package (bin source
     = src/main.rs + src/vis_cli.rs inside the lib crate).
  3. MNF5 lacks a one-call encode (the 4-step pipeline is the only public
     path; MNF7/MNF8 have one-call fns). Inconsistent convenience surface.
  4. Two unrelated `DecodeLimits` structs (codec::, mnf6::) — same intent,
     different types; consumers import both.
  5. Import name vs package name (manifold vs manifold-codec) documented
     in lib.rs + README — the #1 stumble, already handled.

## v1.4 work list (from this audit)

- [package] exclude: testdata/, scripts/cy01_torus.py, .github, .gitlab-ci;
  move the CLI bin to a thin wrapper feature or accept source-ship (choose).
- One-call `encode_mnf5(pixels, dims, use_geom) -> Result<(header+payload)>`
  + unify or alias DecodeLimits (alias = non-breaking; unify = 2.0).
- Criterion benches + a make-bench target that includes the WebP/JXL arms
  when the tools exist, and SKIP with an explicit note when they don't.
- THEN the announcement (pillars 1-3 all green).

## Release requirements (the milestone definition)

- Mathematical correctness: exact reconstruction (every suite, packaged
  tree), independently-tested decoding (refdecode differential suite),
  integer-only decode arithmetic (README contract).
- Scientific credibility: reproducible comparisons vs PNG/WebP/JXL with
  complete bitstream costs — internal now, external one-command next.
- Developer usability: stable API (semver-checks advisory → blocking),
  examples, docs, green CI — green as of run 40416496481.
