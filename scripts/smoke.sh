#!/usr/bin/env bash
# Smoke E2E: builds image, brings the stack up, sends every payload from
# resources/example-payloads.json, and compares the returned fraud_score
# against the brute-force expected count produced by `expected.rs`.
#
# Usage: bash scripts/smoke.sh
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -f resources/references.json.gz ]; then
    cp ../rinha-2026-rust/resources/references.json.gz resources/references.json.gz
fi
if [ ! -f resources/example-payloads.json ]; then
    cp ../rinha-2026-rust/resources/example-payloads.json resources/example-payloads.json
fi

echo "[smoke] computing expected counts via brute-force..."
cargo run --quiet --release -p rust-build --bin expected -- resources/example-payloads.json \
    > data/expected.json

echo "[smoke] docker compose build..."
docker compose build

echo "[smoke] docker compose up -d..."
docker compose up -d

cleanup() { docker compose down -v >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "[smoke] waiting for /ready..."
for i in $(seq 1 60); do
    if curl -fsS http://localhost:9999/ready >/dev/null 2>&1; then
        echo "[smoke]   ready after ${i}s"
        break
    fi
    sleep 1
    if [ "$i" = "60" ]; then
        echo "[smoke] timeout waiting for /ready" >&2
        exit 1
    fi
done

# A fraud_score returned by the API maps back to the count via:
#   score = idx / 5  (where idx is in 0..=5).
# We compare that against the brute-force expected_count.
mismatch=0
total=0
while IFS= read -r line; do
    id=$(echo "$line" | jq -r '.id')
    exp=$(echo "$line" | jq -r '.expected_count')
    payload=$(jq --arg id "$id" '.[] | select(.id==$id)' resources/example-payloads.json)
    resp=$(curl -fsS -X POST http://localhost:9999/fraud-score \
        -H 'Content-Type: application/json' \
        --data "$payload")
    score=$(echo "$resp" | jq -r '.fraud_score')
    got_count=$(awk -v s="$score" 'BEGIN { printf "%d", (s*5)+0.5 }')
    total=$((total+1))
    if [ "$got_count" != "$exp" ]; then
        mismatch=$((mismatch+1))
        echo "[smoke] $id: expected=$exp got=$got_count (score=$score)"
    fi
done < <(jq -c '.[]' data/expected.json)

echo "[smoke] $mismatch / $total mismatches"
pct=$(awk -v m="$mismatch" -v t="$total" 'BEGIN { if (t==0) print 0; else printf "%.2f", (m*100.0)/t }')
echo "[smoke] mismatch rate: ${pct}%"

# Allow up to 2% (the IVF approximation can disagree with brute force on edges).
limit=$(awk -v t="$total" 'BEGIN { printf "%d", (t*0.02)+0.999 }')
if [ "$mismatch" -gt "$limit" ]; then
    echo "[smoke] FAIL: mismatch > 2%" >&2
    exit 2
fi
echo "[smoke] OK"
