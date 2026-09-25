#!/usr/bin/env bash
# Fetch the monthly DB-IP Lite country database (CC BY 4.0) for per-country metrics.
# The server reads it at startup; see doc/METRICS.md.
set -euo pipefail

cd "$(dirname "$0")/.."
dest=${1:-data/geoip/dbip-country-lite.csv}
stamp="$dest.month"

months=("$(date -u +%Y-%m)" "$(date -u -d "$(date -u +%Y-%m-01) -1 month" +%Y-%m)")

mkdir -p "$(dirname "$dest")"
tmp=$(mktemp "$dest.XXXXXX")
trap 'rm -f "$tmp"' EXIT

# The new month's file appears a few days in; fall back to the previous one.
for month in "${months[@]}"; do
    if [[ -s "$dest" && -f "$stamp" && $(cat "$stamp") == "$month" ]]; then
        echo "geoip: $dest is current ($month)"
        exit 0
    fi
    url="https://download.db-ip.com/free/dbip-country-lite-$month.csv.gz"
    if curl -fsSL --retry 2 "$url" | gunzip > "$tmp" && [[ $(wc -l < "$tmp") -gt 100000 ]]; then
        chmod 644 "$tmp"
        mv "$tmp" "$dest"
        printf '%s\n' "$month" > "$stamp"
        echo "geoip: fetched $month ($(wc -l < "$dest") ranges)"
        exit 0
    fi
done

echo "geoip: download failed" >&2
exit 1
