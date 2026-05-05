# Rinha 2026 — PHP + Swoole + Rust

**Vision:** Backend de detecção de fraude em transações de cartão para a Rinha de Backend 2026, escrito em PHP/Swoole no caminho de I/O e em Rust (extensão Zend nativa) no caminho hot, com pontuação final igual ou superior aos dois primeiros colocados (~+5500 a +6000).
**For:** competição Rinha de Backend 2026 — avaliada em Mac Mini 2014 (Haswell, AVX2), Ubuntu 24.04, com k6.
**Solves:** atender o endpoint `POST /fraud-score` com p99 ≤ 1 ms e taxa de falhas (FP+FN+Err) próxima de zero, dentro de 1 CPU e 350 MB de RAM, usando uma stack pouco convencional (PHP) sem comprometer a latência.

## Goals

- p99 da carga oficial `≤ 1 ms` (teto +3000 do score de latência).
- Taxa ponderada de erros `ε ≤ 0,001` e `failure_rate < 1%` (próximo do teto +3000 de detecção).
- `final_score ≥ +5500` na avaliação oficial; aceitável a partir de `+5000`.
- Build local reproduzível com um único `docker compose up --build` (publicação de imagem fica para depois — fora do escopo do MVP).

## Tech Stack

**Core:**
- Linguagem aplicação (I/O): PHP 8.3 (versão flutuante na tag `8.3-cli-bookworm-slim`) com OPcache + JIT tracing.
- Runtime async PHP: ext-swoole 5.x (modo `SWOOLE_BASE`, listener UDS).
- Linguagem hot path: Rust 1.83 stable, edition 2021.
- Bridge PHP↔Rust: extensão Zend nativa via [`ext-php-rs`](https://github.com/davidcole1340/ext-php-rs) (cdylib).
- Load balancer: HAProxy 2.9 (`mode tcp`, `balance roundrobin`, upstream UDS).
- Comunicação LB↔API: Unix Domain Sockets em volume `tmpfs`.

**Key dependencies (Rust):**
- `ext-php-rs` — bridge para Zend.
- `memchr` — busca de substrings no parser JSON manual.
- `mimalloc` — allocator de baixa fragmentação para o crate.
- `libc` — chamadas `mmap`/`madvise` diretas para o índice.
- `flate2` (apenas no `build_index`) — descompactar `references.json.gz`.

## Scope

**v1 includes:**
- `POST /fraud-score` retornando `{approved, fraud_score}` conforme spec da Rinha.
- `GET /ready` retornando 200 quando o índice estiver mmapeado e a extensão carregada.
- Pipeline Rust completo: parse JSON manual, vetorização 14d, IVF k-NN AVX2, contagem de fraudes.
- Índice IVF gerado em build-time a partir de `references.json.gz` (3M vetores).
- 6 respostas JSON pré-computadas (`fraud_score ∈ {0.0, 0.2, 0.4, 0.6, 0.8, 1.0}`).
- `docker-compose.yml` com 1 LB + 2 APIs respeitando 1 CPU / 350 MB.
- `references.json.gz` copiado para o build context (não fica na imagem final — descartado depois do stage `index-builder`).

**Explicitly out of scope:**
- Banco de dados ou cache externo (Redis, Postgres, etc.) — não há estado entre requisições.
- TLS/HTTPS — não exigido pela Rinha.
- Métricas/observabilidade em runtime — apenas logs de erro mínimos.
- Múltiplas regiões / HA real — competição single-host.
- Treinar modelo próprio — algoritmo é fixo (k=5, distância euclidiana, threshold 0,6).
- Brute-force exato sobre 3M — incompatível com p99=1 ms na CPU alvo.

## Constraints

- **Recursos:** soma de `cpus` ≤ 1.00 e soma de `memory` ≤ 350 MB no `docker-compose.yml`.
- **Topologia:** mínimo 1 LB + 2 instâncias de API; LB sem lógica de negócio.
- **Imagens:** públicas, linux/amd64, modo de rede `bridge` (sem `host`/`privileged`).
- **Porta:** LB escuta em `9999` na máquina host.
- **CPU alvo:** Haswell (AVX2 + FMA disponíveis, sem AVX-512).
- **Imutabilidade do dataset:** `references.json.gz`, `mcc_risk.json`, `normalization.json` não mudam — todo pré-processamento é em build-time.
- **Honestidade:** proibido usar `test/test-data.json` para popular o índice ou fazer lookup direto.
