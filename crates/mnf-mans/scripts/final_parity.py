#!/usr/bin/env python3
"""Final release-candidate parity, both phases + Hubble, vs ALL frozen manifests."""
import hashlib, json, os, subprocess, sys
from pathlib import Path

ROOT = Path('/Users/jonbethea/projects/blacktech/manifold')
sys.path.insert(0, str(ROOT / 'crates/mnf-mans/scripts'))
from mnf13_r6_extract import cli_encode
import numpy as np
from PIL import Image

LOCK = [m['file'] for m in json.loads((ROOT / 'research/manifests/corpus.lock').read_text())['files']]
NONMIX = ["testdata/gradient_320x240.png", "testdata/heldout/blob_224x160.png",
          "testdata/heldout/blocks_192x128.png", "testdata/heldout/plaid_192.png",
          "testdata/heldout/poster_208x144.png", "testdata/noise_256x256.png",
          "testdata/repeat_256x256.png", "testdata/solid_256x256.png"]
CORPUS = LOCK + NONMIX
R6 = json.loads((ROOT / 'research/results/x1l_r6_corpus.json').read_text())
REC = {'A': {f: m['sha'] for f, m in R6['r6_3'].items()},
       'B': {f: m['sha'] for f, m in R6['r6_4'].items()}}


def run_corpus(tag):
    mism, total, recon = [], 0, 0
    for rel in CORPUS:
        blob = cli_encode(rel, profile=('mnf8', 'maximum'))
        total += len(blob)
        h = hashlib.sha256(blob).hexdigest()[:32]
        if h != REC[tag][rel]:
            mism.append({'file': rel, 'got': h})
        Path('/tmp/fp.mnf').write_bytes(blob)
        r = subprocess.run(['target/release/manifold', 'decode', '/tmp/fp.mnf', '-o', '/tmp/fp_dec.png'],
                           cwd=ROOT, capture_output=True, timeout=300)
        dpx = np.asarray(Image.open('/tmp/fp_dec.png').convert('RGB'), np.uint8)
        spx = np.asarray(Image.open(ROOT / rel).convert('RGB'), np.uint8)
        recon += int(spx.shape == dpx.shape and not (spx != dpx).any())
    return {'total': total, 'mismatches': mism, 'recon': recon}


REBUILD = 'export PATH=/opt/homebrew/opt/rustup/bin:$PATH; export RUSTUP_HOME=$HOME/.rustup; cargo build --release'
LINE = "        (BACKEND_MIX7, Coder::Zstd(19)),\n"
TF = ROOT / 'src/mnf8.rs'

# Phase B: current tree (backend 7 present, the release candidate)
resB = run_corpus('B')
# Phase A: toggle backend 7 out
s0 = TF.read_text()
TF.write_text(s0.replace(LINE, ''))
subprocess.run(['bash', '-c', REBUILD], cwd=ROOT, capture_output=True, timeout=560)
resA = run_corpus('A')
# Restore the release candidate exactly + rebuild
TF.write_text(s0)
subprocess.run(['bash', '-c', REBUILD], cwd=ROOT, capture_output=True, timeout=560)
assert TF.read_text() == s0, 'candidates file not restored'
blob = cli_encode('/tmp/hf1_hubble_dec.png', profile=('mnf8', 'maximum'))
hb = hashlib.sha256(blob).hexdigest()[:16]
out = {'phase_A': resA, 'phase_B': resB,
       'hubble_final': {'size': len(blob), 'sha16': hb,
                        'byte_identical': hb == 'b2a8a10915239c99'}}
out['FINAL_PARITY'] = ('PASS' if not resA['mismatches'] and not resB['mismatches']
                       and resA['recon'] == 30 and resB['recon'] == 30
                       and resA['total'] == 810816 and resB['total'] == 768303
                       and hb == 'b2a8a10915239c99' else 'FAIL')
print(json.dumps(out), flush=True)
