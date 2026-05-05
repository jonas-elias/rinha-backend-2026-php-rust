# Fraud Score MVP Tasks

**Design:** `.specs/features/fraud-score-mvp/design.md`
**Status:** Draft

---

## Execution Plan

### Phase 1: Scaffolding (sequencial)
```
T1 → T2
```
Cria a árvore de diretórios e os manifests de build (Cargo.toml, composer/php.ini placeholders, README, .dockerignore). Sem isso nada compila.

### Phase 2: Index builder (paralelo a 3a se desejar)
```
T2 → T3 → T4 → T5
```
Binário `build_index` que ingere `references.json.gz` e produz `data/index.bin`. Bloqueante para qualquer teste end-to-end.

### Phase 3: Extensão Rust (núcleo do hot path) — granular sequencial com [P]
```
            ┌→ T6 (json) ─────┐
T2 → T7 ────┼→ T8 (vector) ───┼→ T10 (knn) ──┐
            └→ T9 (mmap data)─┘               │
                                              ▼
                                            T11 (smoke ext)
```
- T7 cria o esqueleto cdylib + macros `ext-php-rs`. Depois disso T6, T8 e T9 podem ir em paralelo (arquivos diferentes).
- T10 (knn) consome T6+T8+T9.
- T11 valida que `php -m` carrega a extensão e que `rinha_fraud_score("{}")` retorna valor sensato.

### Phase 4: PHP/Swoole + LB (sequencial curto, [P] permitido)
```
T11 → T12 (php server)
T1  → T13 (haproxy.cfg)        [P com T12]
T12 + T13 → T14 (Dockerfile)
T14 → T15 (docker-compose.yml)
```

### Phase 5: Validação local + tuning
```
T15 → T16 (smoke E2E) → T17 (k6 harness) → T18 (tune nprobe/quant)
T17 → T19 (tune php/swoole)              [P com T18]
T18 + T19 → T20 (sanity full run)
```

### Phase 6: (Deferida) Submissão oficial

A publicação de imagem no Docker Hub, criação da branch `submission` e abertura de PR/issue na Rinha foram **explicitamente deferidas pelo usuário**. Quando reativadas, virão como nova feature `submission-package` em `.specs/features/`.

---

## Greenfield testing strategy

Como `.specs/codebase/TESTING.md` ainda não existe (greenfield), adotamos:

| Layer | Test type | Comando de gate |
|---|---|---|
| Rust hot path (`json`, `vector`, `knn`) | unit (cargo test) | `cargo test --release --features test-bench` (gate quick) |
| Rust integração (extensão carregada) | integration (PHP smoke) | `php -d extension=./target/release/librinha_rs.so -r 'echo rinha_fraud_score(...);'` |
| Stack completo | e2e (k6) | `bash scripts/run-local.sh` (gate full) |
| Docker compose lint | none / static | `docker compose config -q` (gate quick) |

Toda task que toca código Rust com requisito "unit" deve incluir o teste correspondente no `Done when`. Tasks de Docker/PHP/HAProxy usam o gate static/E2E.

---

## Task Breakdown

### T1: Scaffolding do diretório do projeto

**What:** Criar `rinha-2026-php-rust/` com subpastas `php/`, `rust-ext/src/`, `rust-build/src/`, `data/` (vazia), `scripts/`. Adicionar `README.md` (visão de 1 parágrafo + comandos `docker compose up`), `.gitignore` (`/target/`, `/data/index.bin`, `vendor/`, `*.so`), `.dockerignore`.
**Where:** `rinha-2026-php-rust/` raiz e subdiretórios.
**Depends on:** —
**Reuses:** estrutura de `rinha-2026-rust/`.
**Requirement:** TOPO-04 (estrutura), pré-requisito de todas.

**Tools:**
- MCP: `filesystem` (built-in)
- Skill: NONE

**Done when:**
- [ ] Árvore criada conforme design.
- [ ] `README.md` lista comandos de build/run/test.
- [ ] `.gitignore` ignora artefatos de build.

**Tests:** none
**Gate:** none (apenas inspeção visual)

---

### T2: Manifests de build (Cargo workspaces)

**What:** Criar `rust-ext/Cargo.toml` (`crate-type=["cdylib"]`, deps `ext-php-rs`, `mimalloc`, `memchr`, `libc`; `[profile.release]` com `lto=fat`, `codegen-units=1`, `panic=abort`, `strip=true`, `opt-level=3`). Criar `rust-build/Cargo.toml` (binário `build_index`, deps `flate2`, `serde`, `serde_json`). Criar `Cargo.toml` raiz com `workspace.members = ["rust-ext", "rust-build"]`.
**Where:** `rinha-2026-php-rust/Cargo.toml`, `rust-ext/Cargo.toml`, `rust-build/Cargo.toml`, `rust-ext/build.rs`.
**Depends on:** T1
**Reuses:** `[profile.release]` de `rinha-2026-rust/Cargo.toml`.
**Requirement:** suporte indireto a FRAUD-04, BUILD-01.

**Tools:**
- MCP: `filesystem`, `context7` (consultar versão atual de `ext-php-rs`).
- Skill: NONE

**Done when:**
- [ ] `cargo metadata` retorna sem erro.
- [ ] `cargo check -p rust-ext` compila esqueleto vazio.
- [ ] `cargo check -p rust-build` compila esqueleto vazio.
- [ ] Versões fixadas (sem `*`); `ext-php-rs` validado contra PHP 8.3 via Context7.

**Tests:** none
**Gate:** quick (`cargo check --workspace`)

---

### T3: `build_index` — loader do `references.json.gz`

**What:** Em `rust-build/src/build_index.rs`, portar `load_dataset()` do projeto Rust de referência: abrir `resources/references.json.gz`, descompactar via `flate2::read::GzDecoder`, parsear streaming com `serde_json::Deserializer::from_reader` e visitor que coleta `Vec<[f32;14]>` + `Vec<u8>` (1=fraud, 0=legit).
**Where:** `rust-build/src/build_index.rs` (função `load_dataset`).
**Depends on:** T2
**Reuses:** `rinha-2026-rust/src/build_index.rs` (linhas 52-87).
**Requirement:** BUILD-01.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Função `load_dataset()` retorna `(Vec<[f32;14]>, Vec<u8>)` com 3M elementos quando rodada contra o arquivo oficial.
- [ ] Teste unit (com `example-references.json` descompactado) confirma contagem e labels.

**Tests:** unit
**Gate:** quick (`cargo test -p rust-build --release`)

---

### T4: `build_index` — k-means k=4096, 25 iter

**What:** Portar `kmeans_plus_plus_init`, `assign_parallel` (threads), `update_centroids`, `dist_sq` 1:1 do projeto de referência. Seed fixa `0xdeadbeef_cafebabe`.
**Where:** `rust-build/src/build_index.rs`.
**Depends on:** T3
**Reuses:** `rinha-2026-rust/src/build_index.rs` (linhas 89-214).
**Requirement:** FRAUD-04 (qualidade IVF), BUILD-01.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Em dataset de 100k vetores (subamostra), 25 iterações convergem (changed < 0.1% antes do limite).
- [ ] Teste unit com seed fixa garante reprodutibilidade dos centroides em duas execuções.

**Tests:** unit
**Gate:** quick

---

### T5: `build_index` — serialização do `index.bin` IVF

**What:** Portar `write_index` 1:1: header `IVF1`+n+k+d, `centroids` SoA `c[d*k+ci]` em f32, `block_offsets[k+1]` em u32, `labels[padded_n]` u8, `blocks[total_blocks*112]` i16 (14 dim × 8 slot SoA, scale `1e-4` via `quantize`). **Diferença vs. referência:** gravar em `data/index.bin` *sem* gzip (será mmapeado direto, gzip atrapalha).
**Where:** `rust-build/src/build_index.rs`.
**Depends on:** T4
**Reuses:** `rinha-2026-rust/src/build_index.rs` (linhas 216-316).
**Requirement:** BUILD-01, BUILD-02.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `cargo run --release --bin build_index -- resources/references.json.gz data/index.bin` produz arquivo `< 90 MB`.
- [ ] SHA-256 do `index.bin` é estável entre duas execuções consecutivas com mesma seed (BUILD-01).
- [ ] Teste unit mini lê o header de volta e confirma magic + n + k + d.

**Tests:** unit
**Gate:** quick

---

### T6: Parser JSON manual da extensão

**What:** Em `rust-ext/src/json.rs` implementar `parse_payload(body: &[u8]) -> Tx<'_>` extraindo todos os campos descritos no design: `transaction.amount`, `installments`, `requested_at`; `customer.avg_amount`, `tx_count_24h`, `known_merchants`; `merchant.id/mcc/avg_amount`; `terminal.is_online/card_present/km_from_home`; `last_transaction` (objeto ou `null`). Usar `memchr::memmem::find` por substring de chave (rápido para JSON pequeno e sem aninhamento ambíguo). Implementar `parse_f32`, `parse_u32`, `parse_iso_hour_weekday` à mão. Tratar `last_transaction: null` retornando `minutes_since_last=-1`, `km_from_last=-1.0`.
**Where:** `rust-ext/src/json.rs`.
**Depends on:** T7 (esqueleto da extensão precisa existir para o `mod`).
**Reuses:** `rinha-2026-rust/src/json.rs` adaptado.
**Requirement:** FRAUD-01, FRAUD-03, EDGE-01.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Função compila e roda.
- [ ] Testes unit cobrindo: payload completo, `last_transaction: null`, `installments=0`, MCC inválido, ordem de campos diferente, espaços/quebras de linha extras, body com chaves desconhecidas no meio.
- [ ] Body sintaticamente quebrado retorna `Err`/sentinel default (definido em design para virar resposta `0.0`), não panica.

**Tests:** unit
**Gate:** quick (`cargo test -p rust-ext --release`)

---

### T7: Esqueleto da extensão Rust (`ext-php-rs`)

**What:** Em `rust-ext/src/lib.rs` declarar módulos (`mod data; mod json; mod vector; mod knn; mod response;`), `#[php_module]` registrando `rinha_fraud_score(body: &[u8]) -> u32` e função stub que retorna `0`. Configurar `build.rs` conforme docs do `ext-php-rs`. Ajustar `MiMalloc` como `#[global_allocator]`.
**Where:** `rust-ext/src/lib.rs`, `rust-ext/build.rs`.
**Depends on:** T2
**Reuses:** docs `ext-php-rs` (Context7).
**Requirement:** FRAUD-01 (infra).

**Tools:**
- MCP: `filesystem`, `context7`
- Skill: NONE

**Done when:**
- [ ] `cargo build --release -p rust-ext` produz `target/release/librinha_rs.so`.
- [ ] `php -d extension=$(pwd)/target/release/librinha_rs.so -r "echo rinha_fraud_score('{}');"` imprime `0` sem segfault.

**Tests:** integration (smoke PHP)
**Gate:** quick

---

### T8: Vetorização 14d (`vector.rs`)

**What:** Em `rust-ext/src/vector.rs` implementar `vectorize(tx: &Tx) -> [f32; 14]` com:
- LUTs estáticas para `installments` (0..=12), `hour` (0..=23 → /23), `weekday` (0..=6 → /6), `tx_count_24h` (0..=20).
- `mcc_risk` via `match` literal (10 entradas + default 0.5) — embeddado a partir de `resources/mcc_risk.json` em build-time (`include_str!` + parse).
- `unknown_merchant` via varredura linear sobre `known_merchants` (lista pequena).
- Constantes `max_amount=10000`, `max_km=1000`, `max_merchant_avg_amount=10000`, `amount_vs_avg_ratio=10`, `max_minutes=1440` como `const f32`.
- Retornar `-1.0` em índices 5/6 quando `tx.minutes_since_last == -1`.
**Where:** `rust-ext/src/vector.rs`.
**Depends on:** T7
**Reuses:** `rinha-2026-rust/src/vector.rs` adaptado.
**Requirement:** FRAUD-03, FRAUD-05, EDGE-02..04.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Testes unit usando os dois exemplos canônicos da spec (legit do `tx-1329056812` e fraud do `tx-3330991687`) batem com os vetores listados em `REGRAS_DE_DETECCAO.md` com tolerância 1e-4.
- [ ] Teste de clamping: `amount=99999` → 1.0; `installments=99` → 1.0; `tx_count_24h=999` → 1.0.
- [ ] Teste de sentinela: `last_transaction=None` → `vec[5]==-1.0 && vec[6]==-1.0`.

**Tests:** unit
**Gate:** quick

---

### T9: mmap do `index.bin` (`data.rs`)

**What:** Em `rust-ext/src/data.rs` implementar `init() -> &'static Dataset` (lazy via `OnceLock`):
1. Ler env `INDEX_PATH` (default `/app/data/index.bin`).
2. `open(O_RDONLY)` + `mmap(NULL, len, PROT_READ, MAP_SHARED|MAP_POPULATE, fd, 0)`.
3. Validar `IVF1` magic, ler header, calcular slices.
4. Construir `Dataset { k, n, centroids, offsets, labels, blocks }` apontando para o mmap.
5. Chamar `madvise(MADV_WILLNEED)` no range completo.
6. `OnceLock::get_or_init` garante uma única inicialização por processo.
**Where:** `rust-ext/src/data.rs`.
**Depends on:** T7, T5 (formato do arquivo definido).
**Reuses:** `rinha-2026-rust/src/data.rs`.
**Requirement:** FRAUD-04, READY-02, EDGE-06.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `dataset()` retorna `&'static Dataset` válido após `php_module_init`.
- [ ] Erro de magic / arquivo ausente faz a init falhar com mensagem clara (não panic in handler).
- [ ] Teste unit em `tempfile`: escreve um `index.bin` mínimo, mmapeia, valida ponteiros.

**Tests:** unit
**Gate:** quick

---

### T10: IVF k-NN AVX2 (`knn.rs`)

**What:** Portar 1:1 `rinha-2026-rust/src/knn.rs`:
- `compute_centroid_dists` AVX2 fmadd.
- `top_n_from_dists::<N>` insertion-sort com mask SIMD.
- `scan_blocks` com `dim_pair!` macro, threshold pruning após dim 8.
- `knn5_ivf`: nprobe=8 → se count ∈ {2,3} refazer com nprobe=24.
- `warmup()` rodando 500 queries pseudoaleatórias para preencher caches/branch predictors.
- Função pública `knn5_fraud_count(query: &[f32; 14], ds: &Dataset) -> u8`.
**Where:** `rust-ext/src/knn.rs`.
**Depends on:** T9, T8
**Reuses:** `rinha-2026-rust/src/knn.rs` linhas 1-285.
**Requirement:** FRAUD-02, FRAUD-04, EDGE-05.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Compila com `RUSTFLAGS="-C target-cpu=haswell"`.
- [ ] Teste unit: contra um índice mini de 8 vetores, retorna a contagem esperada para queries pré-calculadas.
- [ ] Microbenchmark (criterion ou `std::hint::black_box`) imprime `< 100 µs/query` em laptop x86_64 moderno (estimativa de referência; alvo final é 1 ms full-stack).

**Tests:** unit
**Gate:** quick

---

### T11: Integração da extensão `rinha_fraud_score` end-to-end (Rust)

**What:** Em `rust-ext/src/lib.rs`, substituir o stub por: `let tx = json::parse_payload(body); let v = vector::vectorize(&tx); let count = knn::knn5_fraud_count(&v, data::dataset()); count as u32`. No `php_module_init` chamar `data::init()` e `knn::warmup()`.
**Where:** `rust-ext/src/lib.rs`.
**Depends on:** T6, T8, T10
**Reuses:** —
**Requirement:** FRAUD-01, FRAUD-02, FRAUD-04, EDGE-01.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `php -d extension=… -r '...'` chamando `rinha_fraud_score` com os dois payloads canônicos retorna `0` (legit) e `5` (fraud) respectivamente.
- [ ] Body inválido retorna valor sensato (0) sem crashar o processo PHP.
- [ ] Logs `eprintln!` mostram tempo de mmap e warmup uma única vez no `php -m`.

**Tests:** integration (smoke PHP)
**Gate:** quick

---

### T12: `php/server.php` Swoole HTTP UDS

**What:** Implementar conforme design: `Swoole\Http\Server('unix:'.$sock, 0, SWOOLE_BASE)`, `worker_num=1`, `reactor_num=1`, handler `on('request')` com:
- `if uri==='/ready' && method==='GET'`: status 200, end ''.
- `if uri==='/fraud-score' && method==='POST'`: `$idx = rinha_fraud_score($req->rawContent()); $resp->header('Content-Type','application/json'); $resp->end($RES[$idx])`.
- Else: 404.
Ler `SOCK` de env. `php.ini` companheiro com `extension=rinha_rs`, `opcache.enable_cli=1`, `opcache.jit=tracing`, `opcache.jit_buffer_size=32M`, `memory_limit=64M`.
**Where:** `php/server.php`, `php/php.ini`.
**Depends on:** T11
**Reuses:** padrões de Swoole UDS server.
**Requirement:** FRAUD-01, READY-01, READY-02, EDGE-01.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `php -c php/php.ini -d extension_dir=… php/server.php` sobe e responde via `curl --unix-socket /tmp/rinha.sock http://localhost/ready` → 200.
- [ ] `curl --unix-socket /tmp/rinha.sock -X POST … /fraud-score` com payload canônico devolve string esperada exatamente.

**Tests:** integration (smoke local sem docker)
**Gate:** quick

---

### T13: `haproxy.cfg`

**What:** Config: `global` (`maxconn 4096`, `nbthread 1`, `tune.bufsize 16384`); `defaults` (`mode tcp`, `timeout connect 1s`, `timeout client 5s`, `timeout server 5s`); `frontend rinha_in` `bind *:9999` `default_backend rinha_apis`; `backend rinha_apis` `balance roundrobin` + `server srv1 unix@/sockets/api1.sock` e `srv2 unix@/sockets/api2.sock` (`check` desligado para evitar overhead).
**Where:** `haproxy.cfg`.
**Depends on:** T1
**Reuses:** docs HAProxy 2.9 (Context7/web).
**Requirement:** TOPO-03, TOPO-04.

**Tools:**
- MCP: `filesystem`, `context7`
- Skill: NONE

**Done when:**
- [ ] `haproxy -f haproxy.cfg -c` (validação de sintaxe) passa.
- [ ] Smoke local: `haproxy` standalone + dois `nc -lU` simulados recebem requisições alternadas.

**Tests:** none (config validation only)
**Gate:** static (`haproxy -c`)

---

### T14: `Dockerfile` multi-stage

**What:** Três stages:
1. `FROM rust:1.83-slim AS index-builder` — copia `resources/`, `Cargo.toml`s, `rust-build/`; `RUN cargo run --release -p rust-build --bin build_index resources/references.json.gz /out/index.bin`.
2. `FROM rust:1.83-slim AS ext-builder` — instala `php8.3-dev`, `clang`, `libclang-dev`, `pkg-config`; `ENV RUSTFLAGS="-C target-cpu=haswell -C target-feature=+avx2,+fma,+bmi2"`; `RUN cargo build --release -p rust-ext` → `/out/librinha_rs.so`.
3. `FROM php:8.3-cli-bookworm-slim AS runtime` — `RUN pecl install swoole && docker-php-ext-enable swoole`; copia `librinha_rs.so` para `$(php-config --extension-dir)/rinha_rs.so`; copia `index.bin` para `/app/data/`; copia `php/` para `/app/`; `ENV INDEX_PATH=/app/data/index.bin`; `CMD ["php", "-c", "/app/php.ini", "/app/server.php"]`.
**Where:** `Dockerfile`.
**Depends on:** T12, T13, T11 (ext funcional), T5 (build_index funcional).
**Reuses:** padrões dos dois projetos de referência.
**Requirement:** TOPO-04.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `docker build --platform=linux/amd64 -t rinha-2026-php-rust:dev .` completa sem erro.
- [ ] `docker run --rm rinha-2026-php-rust:dev php -m | grep rinha_rs` retorna a extensão.
- [ ] Tamanho da imagem é monitorado mas não-bloqueante (registro em STATE.md como métrica informativa).

**Tests:** none (validação manual)
**Gate:** build

---

### T15: `docker-compose.yml`

**What:** Serviços `api1`, `api2`, `lb`. APIs com `image: rinha-2026-php-rust:dev`, env `SOCK=/sockets/apiN.sock`, volume `sockets:/sockets`, `cpus: "0.40"`, `memory: "165M"`, `ulimits.nofile: 65535`, `security_opt: [seccomp:unconfined]`. LB com `image: haproxy:2.9-alpine`, monta `./haproxy.cfg`, `cpus: "0.20"`, `memory: "20M"`, `ports: ["9999:9999"]`. Volume `sockets` declarado com `driver: local` + `driver_opts.type=tmpfs`. Network `bridge`.
**Where:** `docker-compose.yml`.
**Depends on:** T14
**Reuses:** `rinha-2026-rust/docker-compose.yml`.
**Requirement:** TOPO-01, TOPO-02, TOPO-03, TOPO-04.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] `docker compose config -q` passa.
- [ ] Soma `cpus` = `1.00`, soma `memory` = `350MB` (calculada via `yq`).
- [ ] `docker compose up -d` sobe os 3 serviços e `curl http://localhost:9999/ready` retorna 200 em ≤ 3 s.

**Tests:** none (smoke)
**Gate:** static + smoke

---

### T16: Smoke E2E (acerto funcional)

**What:** Script `scripts/smoke.sh` que sobe o compose, percorre `resources/example-payloads.json` enviando cada um a `http://localhost:9999/fraud-score` e compara o `approved` retornado com o esperado calculado por k-NN brute-force em Rust standalone (script auxiliar `rust-build/src/bin/expected.rs`). Falha o build se >2% de discrepância.
**Where:** `scripts/smoke.sh`, `rust-build/src/bin/expected.rs`.
**Depends on:** T15
**Reuses:** `resources/example-payloads.json` (oficial).
**Requirement:** FRAUD-02, EDGE-02..05.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Discrepância ≤ 2% em `example-payloads.json` (IVF aproximado, não exato — esperado).
- [ ] Falha clara quando a extensão estiver com bug (ex.: vetor[5/6]≠-1 em null).

**Tests:** integration (E2E)
**Gate:** full

---

### T17: Harness k6 local

**What:** `scripts/run-local.sh` que:
1. Garante que `docker compose up -d` está saudável (`/ready` 200).
2. Copia `rinha-de-backend-2026/test/` para `./bench/` ajustando `target` para `http://localhost:9999`.
3. Roda `k6 run bench/test.js` com `--summary-trend-stats=p(99)` e `--out json=bench/raw.json`.
4. Imprime `bench/results.json` formatado e copia para `bench/history/$(date +%s).json`.
**Where:** `scripts/run-local.sh`.
**Depends on:** T15
**Reuses:** `rinha-de-backend-2026/test/test.js` e `test-data.json`.
**Requirement:** FRAUD-04 (mensuração).

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Script roda fim a fim e gera `results.json`.
- [ ] `final_score` impresso no terminal.
- [ ] Histórico das últimas 10 execuções preservado.

**Tests:** none
**Gate:** smoke

---

### T18: Tuning de `nprobe` e quantização [P com T19]

**What:** Em `bench/tuning.md` registrar matriz de `nprobe ∈ {6,8,12,16,24}` × `scale ∈ {1e-4, 1/8192}` × `nprobe_fallback ∈ {16,24,32}`. Para cada combinação: rebuildar a extensão (e o índice quando `scale` muda), rodar T17 três vezes, registrar p99 e `final_score` médio. Escolher o melhor.
**Where:** `bench/tuning.md`, `rust-ext/src/knn.rs` (consts ajustáveis via env).
**Depends on:** T17
**Reuses:** —
**Requirement:** FRAUD-04.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Tabela em `bench/tuning.md` preenchida com ≥ 9 combinações.
- [ ] Configuração vencedora codada como default no Cargo (consts em `knn.rs`).
- [ ] p99 mediano local ≤ 2 ms e `failure_rate` < 1%.

**Tests:** none (mensuração)
**Gate:** full

---

### T19: Tuning PHP/Swoole/HAProxy [P com T18]

**What:** Variar:
- `opcache.jit` ∈ {`off`, `tracing`, `function`}.
- `worker_num` ∈ {1, 2} (cuidado: impacta CPU budget).
- `reactor_num` ∈ {1, 2}.
- `socket_buffer_size` (Swoole).
- HAProxy `tune.bufsize` ∈ {8192, 16384, 32768}.
Para cada mudança rodar T17 e registrar p99 em `bench/tuning.md`.
**Where:** `php/php.ini`, `haproxy.cfg`, `bench/tuning.md`.
**Depends on:** T17
**Reuses:** —
**Requirement:** FRAUD-04.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] Tabela com ≥ 6 combinações.
- [ ] Configuração vencedora aplicada como default.

**Tests:** none
**Gate:** full

---

### T20: Sanity full run (3 execuções consecutivas)

**What:** Rodar T17 três vezes seguidas em ambiente limpo (`docker compose down -v && up -d --build`). Validar que `final_score ≥ +5500` em todas. Se não, voltar para T18/T19 e iterar.
**Where:** `bench/history/`.
**Depends on:** T18, T19
**Reuses:** —
**Requirement:** FRAUD-04, success criteria globais.

**Tools:**
- MCP: `filesystem`
- Skill: NONE

**Done when:**
- [ ] 3 execuções consecutivas com `final_score ≥ +5500` registradas em `bench/history/`.
- [ ] STATE.md atualizado com a melhor configuração.

**Tests:** none (mensuração)
**Gate:** full

---

## Pre-Approval Validation

### Check 1 — Granularity

Cada tarefa entrega um único artefato (um arquivo Rust, um arquivo PHP, um YAML, um script). Tasks compostas (parser + vector + knn) foram quebradas em T6, T8, T10. ✅

### Check 2 — Diagram-Definition Cross-Check

| Task | Depends on (texto) | Aparece no diagrama? |
|---|---|---|
| T1 | — | Phase 1 raiz | ✅ |
| T2 | T1 | Phase 1 → Phase 2 | ✅ |
| T3 | T2 | T2 → T3 | ✅ |
| T4 | T3 | T3 → T4 | ✅ |
| T5 | T4 | T4 → T5 | ✅ |
| T6 | T7 | T7 → T6 (paralelo a T8/T9) | ✅ |
| T7 | T2 | T2 → T7 | ✅ |
| T8 | T7 | T7 → T8 | ✅ |
| T9 | T7, T5 | T7 → T9; T5 alimenta formato | ✅ (alimentação implícita por arquivo) |
| T10 | T9, T8 | T8+T9 → T10 | ✅ |
| T11 | T6, T8, T10 | T10 → T11 | ✅ (T6/T8 já no caminho) |
| T12 | T11 | T11 → T12 | ✅ |
| T13 | T1 | T1 → T13 (paralelo a T12) | ✅ |
| T14 | T12, T13, T11, T5 | T12+T13 → T14 | ✅ |
| T15 | T14 | T14 → T15 | ✅ |
| T16 | T15 | T15 → T16 | ✅ |
| T17 | T15 | T15 → T17 (após T16) | ✅ |
| T18 | T17 | T17 → T18 | ✅ |
| T19 | T17 | T17 → T19 (paralelo) | ✅ |
| T20 | T18, T19 | T18+T19 → T20 | ✅ |

Resultado: ✅ todas as deps batem.

### Check 3 — Test Co-location

Greenfield: regra adotada acima. Verificação:

| Task | Code layer | Tipo de teste declarado | OK? |
|---|---|---|---|
| T3 | Rust unit (load) | unit | ✅ |
| T4 | Rust unit (kmeans) | unit | ✅ |
| T5 | Rust unit (serializer) | unit | ✅ |
| T6 | Rust unit (json) | unit | ✅ |
| T7 | Integration smoke | integration | ✅ |
| T8 | Rust unit (vector) | unit | ✅ |
| T9 | Rust unit (mmap) | unit | ✅ |
| T10 | Rust unit (knn) | unit | ✅ |
| T11 | Integration smoke | integration | ✅ |
| T12 | Integration smoke | integration | ✅ |
| T13 | Static config | none/static | ✅ |
| T14 | Build smoke | none/build | ✅ |
| T15 | Static + smoke | none/static | ✅ |
| T16 | E2E | integration (E2E) | ✅ |
| T17 | Harness | none | ✅ (script de outras tasks) |
| T18-T20 | Mensuração | none | ✅ |

Resultado: ✅.

---

## Tools and Skills (per task)

Default tooling adotado:

- `filesystem` (built-in) para todas as tasks que tocam arquivos.
- `context7` quando a task envolver biblioteca externa (T2 ext-php-rs, T13 HAProxy).
- `gh` MCP / GitHub CLI: **não usado no MVP** (publicação e submissão deferidas).
- Sub-agentes (Task tool): preferir agente `general-purpose` em T6, T8, T10 (volume de código portado), `task` agent em T18/T19/T20 (executar k6 e capturar resultados).
- Nenhum skill adicional além do TLC SDD e do `find-skills` já carregado.

---

## Open Questions Before Execution

1. Confirmação de que copiar `references.json.gz` (~16 MB) para o build context é aceitável.
   - **Resposta capturada:** sim, desde que isolado no stage `index-builder` e não vá para a imagem runtime — atendido pela arquitetura multi-stage do Dockerfile (T14).
2. PHP version pin: `8.3.x` exato ou flutuante?
   - **Resposta capturada:** flutuante (`php:8.3-cli-bookworm-slim` sem patch lock).
3. Publicação de imagem em registry público?
   - **Resposta capturada:** fora do escopo do MVP. Tasks T21–T23 originais foram deferidas para uma feature futura `submission-package`.
