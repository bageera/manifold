# Lossless benchmark — identical decoded RGB8 source (README §7)

Sources are the synthetic corpus PNGs; every codec reads the same PNG and
must reproduce it byte-exactly in RGB8 (MNF5 via `verify`, PNG by
construction, JXL/WebP via decode-to-PNG + Pillow `tobytes()` compare).
Peak RSS in KB (macOS `/usr/bin/time -l`). Regenerate with
`python3 scripts/bench.py` (dev-only Pillow dependency, not used by the codec).

Interpretation notes (honest losses included): WebP-lossless's palette mode
dominates all synthetic classes; MNF5 leads JXL and PNG on the
repeat-pattern class and loses to plain deflate on solid tiles (its
documented proxy-cost caveat). MNF5's peak RSS (~4 MB here) is the lowest
of all codecs measured — the atlas is metadata-light; JXL's spikes to
48 MB on the screenshot reflect its modular pipeline, not our encoder.

| image | codec | bytes | peak RSS KB | exact |
|---|---|---|---|---|
| gradient_320x240.png | mnf5-no-atlas | 222923 | 4016 | yes |
| gradient_320x240.png | mnf5-atlas | 3738 | 3920 | yes |
| gradient_320x240.png | mnf5-geom-atlas | 3887 | 4016 | yes |
| gradient_320x240.png | png | 69512 | - | yes |
| gradient_320x240.png | jxl-lossless | 1603 | 10784 | yes |
| gradient_320x240.png | webp-lossless | 148 | 15344 | yes |
| noise_256x256.png | mnf5-no-atlas | 196929 | 3744 | yes |
| noise_256x256.png | mnf5-atlas | 196929 | 3888 | yes |
| noise_256x256.png | mnf5-geom-atlas | 196929 | 3872 | yes |
| noise_256x256.png | png | 196947 | - | yes |
| noise_256x256.png | jxl-lossless | 216172 | 11904 | yes |
| noise_256x256.png | webp-lossless | 196688 | 14352 | yes |
| repeat_256x256.png | mnf5-no-atlas | 3918 | 3536 | yes |
| repeat_256x256.png | mnf5-atlas | 1760 | 3712 | yes |
| repeat_256x256.png | mnf5-geom-atlas | 1760 | 3696 | yes |
| repeat_256x256.png | png | 140074 | - | yes |
| repeat_256x256.png | jxl-lossless | 7261 | 10864 | yes |
| repeat_256x256.png | webp-lossless | 358 | 13344 | yes |
| screenshot_640x400.png | mnf5-no-atlas | 3854 | 6560 | yes |
| screenshot_640x400.png | mnf5-atlas | 3947 | 6720 | yes |
| screenshot_640x400.png | mnf5-geom-atlas | 3897 | 6752 | yes |
| screenshot_640x400.png | png | 11181 | - | yes |
| screenshot_640x400.png | jxl-lossless | 498 | 49232 | yes |
| screenshot_640x400.png | webp-lossless | 320 | 11328 | yes |
| solid_256x256.png | mnf5-no-atlas | 728 | 3504 | yes |
| solid_256x256.png | mnf5-atlas | 875 | 3648 | yes |
| solid_256x256.png | mnf5-geom-atlas | 875 | 3632 | yes |
| solid_256x256.png | png | 2677 | - | yes |
| solid_256x256.png | jxl-lossless | 49 | 18784 | yes |
| solid_256x256.png | webp-lossless | 44 | 4016 | yes |

# Held-out evaluation corpus (disjoint generators; eval-only, never tuned on)

| image | codec | bytes | peak RSS KB | exact |
|---|---|---|---|---|
| blob_224x160.png | mnf5-no-atlas | 24408 | 3248 | yes |
| blob_224x160.png | mnf5-atlas | 18878 | 3648 | yes |
| blob_224x160.png | mnf5-geom-atlas | 16490 | 3712 | yes |
| blob_224x160.png | png | 32759 | - | yes |
| blob_224x160.png | jxl-lossless | 7786 | 9024 | yes |
| blob_224x160.png | webp-lossless | 7792 | 23840 | yes |
| blocks_192x128.png | mnf5-no-atlas | 2034 | 2912 | yes |
| blocks_192x128.png | mnf5-atlas | 2195 | 3072 | yes |
| blocks_192x128.png | mnf5-geom-atlas | 2382 | 3072 | yes |
| blocks_192x128.png | png | 18184 | - | yes |
| blocks_192x128.png | jxl-lossless | 1837 | 9488 | yes |
| blocks_192x128.png | webp-lossless | 1344 | 17312 | yes |
| document_200x280.png | mnf5-no-atlas | 1412 | 3760 | yes |
| document_200x280.png | mnf5-atlas | 1468 | 3648 | yes |
| document_200x280.png | mnf5-geom-atlas | 1519 | 3968 | yes |
| document_200x280.png | png | 3490 | - | yes |
| document_200x280.png | jxl-lossless | 474 | 17648 | yes |
| document_200x280.png | webp-lossless | 312 | 4048 | yes |
| plaid_192.png | mnf5-no-atlas | 108346 | 3264 | yes |
| plaid_192.png | mnf5-atlas | 107554 | 3424 | yes |
| plaid_192.png | mnf5-geom-atlas | 100402 | 3680 | yes |
| plaid_192.png | png | 63366 | - | yes |
| plaid_192.png | jxl-lossless | 24066 | 10752 | yes |
| plaid_192.png | webp-lossless | 25172 | 25792 | yes |
| poster_208x144.png | mnf5-no-atlas | 3686 | 3040 | yes |
| poster_208x144.png | mnf5-atlas | 1912 | 3152 | yes |
| poster_208x144.png | mnf5-geom-atlas | 1917 | 3440 | yes |
| poster_208x144.png | png | 87348 | - | yes |
| poster_208x144.png | jxl-lossless | 3823 | 7168 | yes |
| poster_208x144.png | webp-lossless | 494 | 3856 | yes |

Held-out interpretation: the geom arm shows its first real held-out wins on
natural-ish content (blob: 16,490 vs atlas 18,878 vs no-atlas 24,408 —
transformed references beat exact ones on rotated gradients), and plaid's
sines also prefer geom (100,402 vs 107,554). Blocks/document/poster show the
honest losses: tile-grid-aligned content favors deflate's own LZ77
(blocks: 2,034 no-atlas vs 2,195 atlas) and WebP palette crushes flat-color
art (poster: 494). JXL leads smooth photographic (blob 7,786). No codec was
tuned on these five images; all values pixel-exact.
