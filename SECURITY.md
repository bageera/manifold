# Security Policy

## Reporting a vulnerability

Report vulnerabilities to **security@nocturnalinc.com**.

Please include:

- A description of the issue and its impact
- Steps to reproduce (PoC files welcome — the MNF5/MNF6/MNF7 decoders are
  the interesting attack surface for malformed streams)
- Your assessment of severity (optional)

You will get an acknowledgment within 72 hours.

## Scope

In-scope:

- The MNF5, MNF6, and MNF7 decoders (`src/codec.rs`, `src/refdecode.rs`,
  `src/mnf6.rs`) — untrusted-input handling: malformed headers, tile
  records, reference validation, decompression bounds, mode/transform
  checking
- The CLI tool (`src/main.rs`, `src/img.rs`) — file handling
- The conformance/fuzz tooling (`scripts/fuzz.py`) when it feeds the decoders

Out of scope:

- Upstream dependencies (report to their maintainers; CC us if the fix
  matters here — zstd/zlib/image-crate issues dominate the real surface)
- Social engineering, physical access, denial-of-wallet

## Handling policy

- Do not run the CLI on adversarial files outside a sandbox; the README
  says the same for the decoders.
- Findings go through coordinated disclosure: private report → fix → fix
  release → contributor credit in the release notes and this file (at
  your option). Target: fix release within 90 days of a confirmed report.

## Supported versions

The frozen formats are MNF5 (since v1.0.0), MNF6 and MNF7 (since v1.1.0).
Security fixes land only on the latest tag; older tags are research
records.