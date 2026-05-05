# Rinha de Backend 2026 — PHP + Swoole + Rust

Detecção de fraude por k-NN (k=5) sobre 3M vetores 14d, servida em **PHP 8.3 / Swoole** com hot path em **Rust** carregado como **extensão Zend nativa** via [`ext-php-rs`](https://github.com/davidcole1340/ext-php-rs).

Topologia: HAProxy (porta `9999`) → 2 instâncias `php server.php` ouvindo em UDS (tmpfs).
Algoritmo: IVF (k-means k=4096, AVX2/FMA) + nprobe=8 com fallback nprobe=24 quando count ∈ {2,3}.

## Comandos

```bash
# build da imagem (gera index.bin no stage index-builder)
docker compose build

# subir
docker compose up -d

# health
curl http://localhost:9999/ready

# fraud score
curl -s -X POST http://localhost:9999/fraud-score \
  -H 'Content-Type: application/json' \
  --data-binary @resources/example-payloads.json

# smoke E2E
bash scripts/smoke.sh

# k6 local
bash scripts/run-local.sh
```

## Estrutura

```
rust-ext/      # cdylib carregado pelo PHP (json, vector, knn, data/mmap)
rust-build/    # binário build_index (gera data/index.bin)
php/           # server.php Swoole + php.ini
haproxy.cfg    # frontend tcp -> backend unix
Dockerfile     # multi-stage: index-builder + ext-builder + runtime
docker-compose.yml
.specs/        # specs TLC SDD
```

Veja `.specs/features/fraud-score-mvp/` para spec, design e tasks.
