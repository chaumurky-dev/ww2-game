#!/usr/bin/env bash
# Download Natural Earth 1:50m data (public domain) into data/.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p data
for n in ne_50m_admin_0_countries ne_50m_admin_1_states_provinces; do
  if [ ! -f "data/$n.shp" ]; then
    curl -fL -o "data/$n.zip" "https://naciscdn.org/naturalearth/50m/cultural/$n.zip"
    unzip -oq "data/$n.zip" -d data
  fi
done
