#!/usr/bin/env bash
# Local k6 harness: brings the stack up, runs k6 against /fraud-score and /ready
# with a sustained 1k req/s, prints aggregated metrics.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v k6 >/dev/null 2>&1; then
    echo "k6 not found; install via 'brew install k6' or https://k6.io" >&2
    exit 1
fi

docker compose up -d --build
trap 'docker compose down -v >/dev/null 2>&1 || true' EXIT

# Wait for ready
for i in $(seq 1 60); do
    if curl -fsS http://localhost:9999/ready >/dev/null 2>&1; then break; fi
    sleep 1
done

PAYLOAD=$(jq -c '.[0]' resources/example-payloads.json)

cat > /tmp/rinha_load.js <<JS
import http from 'k6/http';
import { check } from 'k6';

const PAYLOAD = ${PAYLOAD};

export const options = {
    scenarios: {
        warmup: { executor: 'constant-arrival-rate',
            duration: '5s', rate: 200, timeUnit: '1s',
            preAllocatedVUs: 50, maxVUs: 200,
            exec: 'fraud' },
        main: { executor: 'constant-arrival-rate',
            startTime: '6s', duration: '30s',
            rate: 1000, timeUnit: '1s',
            preAllocatedVUs: 200, maxVUs: 1000,
            exec: 'fraud' },
    },
    thresholds: {
        'http_req_duration{kind:fraud}': ['p(99)<5'],
        'http_req_failed': ['rate<0.001'],
    },
};

export function fraud() {
    const res = http.post('http://localhost:9999/fraud-score',
        JSON.stringify(PAYLOAD),
        { headers: { 'Content-Type': 'application/json' }, tags: { kind: 'fraud' } });
    check(res, { 'status 200': (r) => r.status === 200 });
}
JS

k6 run --quiet /tmp/rinha_load.js
