# Rinha 2026 PHP+Rust — Roadmap

**Status:** Draft

## Milestones

### M1 — Foundation (greenfield scaffold)
- Estrutura de diretórios, Dockerfiles e `.specs/` em vigor.
- `build_index` produzindo `data/index.bin` (IVF k=4096, 25 iter Lloyd).
- Extensão Rust mínima (`fraud_score` retornando 0) carregada em `php -m`.

### M2 — Hot path funcional (feature: `fraud-score-mvp`)
- Parser JSON manual + vetorização 14d + IVF k-NN AVX2.
- Servidor PHP/Swoole respondendo `/fraud-score` e `/ready` via UDS.
- HAProxy round-robin, `docker-compose.yml` dentro dos limites.
- Smoke test passa com 100% de acerto em `example-payloads.json`.

### M3 — Tuning de latência e detecção
- Benchmark local com k6 (test.js), loop de tuning `nprobe`, JIT, buffers.
- Atingir p99 ≤ 2 ms localmente e `failure_rate < 1%`.
- Decisão go/no-go sobre subamostragem do dataset (3M → 200–500k).

### M4 — Submissão oficial *(deferida — fora do escopo do MVP atual)*
- Publicar imagem em registry e preparar branch `submission` ficam reservados para uma feature futura.

## Features (planejadas)

| Feature | Milestone | Status |
|---|---|---|
| `fraud-score-mvp` | M2 | Specified |
| `tuning-loop` | M3 | Pending |
| `submission-package` | M4 | Deferred |
