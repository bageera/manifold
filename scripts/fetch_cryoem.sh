#!/bin/sh
# Fetch one EMPIAR file into /tmp for the cryo-EM research probes.
# Usage: fetch_cryoem.sh <entry-id> <data-dir> <filename>
set -e
EID="$1"; DIR="$2"; FILE="$3"
curl -sS -L -H "User-Agent: manifold-research" \
  -o "/tmp/${FILE}" \
  "https://ftp.ebi.ac.uk/empiar/world_availability/${EID}/data/${DIR%/}/${FILE}"
stat -f%z "/tmp/${FILE}"