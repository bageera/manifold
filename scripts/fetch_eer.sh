#!/bin/sh
# Fetch the EMPIAR-10943 EER corpus file used in docs/eer-probe-10061-10943.md
# (Falcon 4i FoilHole movie, 29 frames, 113.9 MiB) into /tmp.
set -e
curl -sS -L -H "User-Agent: manifold-research" \
  -o /tmp/movie.eer.tif \
  "https://ftp.ebi.ac.uk/empiar/world_availability/10943/data/Tiff/EER/Images-Disc1/GridSquare_11149061/Data/FoilHole_11161627_Data_11149751_11149753_20210911_222712_EER.tif"
stat -f%z /tmp/movie.eer.tif
echo "decode: pip install eerfile pydantic; python -c \"import eerfile; print(eerfile.read('/tmp/movie.eer.tif').shape)\""
