# Fraud Score MVP Specification

## Problem Statement

Construir o backend de detecção de fraude da Rinha 2026 com latência p99 ≤ 1 ms e taxa de erro de detecção próxima de zero, dentro de 1 CPU e 350 MB de RAM, usando PHP/Swoole no I/O e Rust (extensão nativa) no caminho hot. O desafio é compensar o overhead intrínseco de PHP delegando 100% do trabalho computacional ao Rust e eliminando alocações no caminho crítico.

## Goals

- [ ] `final_score ≥ +5500` na avaliação oficial (ideal +6000).
- [ ] `p99 ≤ 1 ms` na carga oficial do k6 (`ramping-arrival-rate` 1→900 req/s em 120 s).
- [ ] `failure_rate < 1%` (FP+FN+Err) e `ε ≤ 0.001` ponderado.
- [ ] Imagem Docker pública linux/amd64 com `docker compose up` reproduzível.

## Out of Scope

| Feature | Reason |
|---|---|
| Banco de dados ou cache externo | Desafio é stateless por requisição. |
| TLS/HTTPS | k6 oficial usa HTTP plain. |
| Métricas/observabilidade runtime | Custa CPU/memória; logs de erro mínimos bastam. |
| Brute-force exato sobre 3M | Inviável com p99=1 ms na CPU Haswell alvo. |
| Treinamento próprio do modelo | Algoritmo (k=5, euclidiana, threshold 0,6) é fixo na spec da Rinha. |
| Mais de 2 instâncias de API | A soma de CPU/memória aperta; 2 instâncias bastam pelo regulamento. |

---

## User Stories

### P1: Endpoint de detecção de fraude ⭐ MVP

**User Story:** Como o avaliador da Rinha (k6), quero enviar uma transação em `POST /fraud-score` e receber a decisão correta em ≤ 1 ms, para que o backend pontue o máximo possível.

**Why P1:** É o único endpoint que conta para o score; sem ele não há submissão.

**Acceptance Criteria:**

1. WHEN o avaliador envia `POST /fraud-score` com payload válido THEN o sistema SHALL responder `200 OK` com corpo `{"approved": <bool>, "fraud_score": <0.0|0.2|0.4|0.6|0.8|1.0>}` em formato exato (sem espaços ou campos extras).
2. WHEN o sistema decide a resposta THEN ela SHALL refletir `approved = (count_fraud / 5) < 0.6`, ou seja: `approved=true` quando ≤2 vizinhos forem fraude e `approved=false` quando ≥3 forem fraude.
3. WHEN o sistema recebe um payload com `last_transaction: null` THEN ele SHALL preencher os índices 5 e 6 do vetor com `-1.0` (sentinela), conforme `REGRAS_DE_DETECCAO.md`.
4. WHEN o avaliador executa o teste oficial de 120 s THEN o `p99` observado SHALL ser `≤ 1 ms` e `failure_rate < 1%`.
5. WHEN um MCC do payload não existe em `mcc_risk.json` THEN o sistema SHALL usar `0.5` como valor de risco padrão.

**Independent Test:** Subir o stack com `docker compose up`, rodar `bash test/run.sh` do repo `rinha-de-backend-2026/` e validar `results.json` (`final_score ≥ +5500`, p99 ≤ 1 ms).

---

### P1: Health check para liberação de carga

**User Story:** Como a engine da Rinha, quero consultar `GET /ready` antes do teste de carga, para garantir que o backend está com índice carregado e pronto antes de disparar o k6.

**Why P1:** Sem `/ready` 200, a engine considera o backend indisponível e o teste falha imediatamente.

**Acceptance Criteria:**

1. WHEN a engine chama `GET /ready` após o `docker compose up` finalizar THEN o sistema SHALL responder `200` em até 1 s.
2. WHEN o índice IVF ainda não foi mmapeado THEN o sistema SHALL responder `503` (ou recusar conexão) até estar pronto.

**Independent Test:** `curl -i http://localhost:9999/ready` retorna `200` após o boot.

---

### P1: Topologia LB + 2 APIs respeitando limites

**User Story:** Como o organizador, quero que o `docker-compose.yml` declare exatamente 1 LB + 2 APIs com soma `≤ 1 CPU` e `≤ 350 MB` em `bridge`, para validar a submissão sem ajustes manuais.

**Why P1:** Submissão fora dos limites é desclassificada antes mesmo do k6 rodar.

**Acceptance Criteria:**

1. WHEN o avaliador soma `deploy.resources.limits.cpus` THEN o total SHALL ser `≤ 1.00`.
2. WHEN o avaliador soma `deploy.resources.limits.memory` THEN o total SHALL ser `≤ 350 MB`.
3. WHEN o tráfego HTTP chega na porta 9999 THEN o LB SHALL distribuir as requisições em round-robin puro entre `api1` e `api2` sem inspecionar payload.
4. WHEN qualquer serviço tenta usar `network_mode: host` ou `privileged: true` THEN o sistema SHALL ser rejeitado (não usar essas opções).

**Independent Test:** Lint do `docker-compose.yml` com `docker compose config | yq` somando os limites.

---

### P2: Reprodutibilidade do índice em build-time

**User Story:** Como mantenedor da solução, quero que `docker build` regenere `index.bin` de forma determinística a partir de `references.json.gz`, para que qualquer máquina x86_64 produza o mesmo binário.

**Why P2:** Importante para CI e reprodutibilidade entre máquinas, não bloqueia MVP.

**Acceptance Criteria:**

1. WHEN o builder executa `cargo run --release --bin build_index` com a mesma seed (`0xdeadbeef_cafebabe`) THEN o `data/index.bin` SHALL ter o mesmo hash SHA-256 entre builds.
2. WHEN `references.json.gz` muda THEN o build SHALL detectar via `cargo` e regerar o índice.

**Independent Test:** Dois builds consecutivos produzem o mesmo SHA-256 de `index.bin`.

---

## Edge Cases

- WHEN o payload chega com `Content-Length` 0 ou body inválido THEN o sistema SHALL responder `400` (vai contar como `Err` no k6 — preferível a chumbar 200 com decisão errada? Não — `Err` pesa 5; resposta inválida pesa 1 ou 3. **Decisão**: tentar parsear ao máximo; quando absolutamente impossível, responder `200` com `approved=true, fraud_score=0.0` para evitar peso 5; **não** introduzir 5xx).
- WHEN o payload contém um MCC desconhecido (não está em `mcc_risk.json`) THEN o sistema SHALL usar `0.5` como risco padrão.
- WHEN `customer.known_merchants` está vazio THEN `unknown_merchant` SHALL ser `1`.
- WHEN `transaction.amount > 10000` THEN o sistema SHALL clamp para `1.0` na dimensão 0.
- WHEN `transaction.installments > 12` THEN clamp para `1.0` na dimensão 1.
- WHEN dois clusters do IVF empatam em distância THEN o sistema SHALL preferir o de menor índice (determinístico via insertion sort estável).
- WHEN a contagem de fraudes na primeira passada com `nprobe=8` ficar em `{2, 3}` (zona ambígua) THEN o sistema SHALL refazer com `nprobe=24` antes de decidir.
- WHEN `index.bin` está corrompido ou faltando no boot THEN o processo SHALL abortar com log claro antes de aceitar conexões (e `/ready` SHALL nunca responder 200).
- WHEN o socket UDS já existe ao iniciar THEN o servidor SHALL `unlink` e recriar antes do `bind`.

---

## Requirement Traceability

| Requirement ID | Story | Phase | Status |
|---|---|---|---|
| FRAUD-01 | P1: Endpoint `/fraud-score` (formato resposta) | Design | Pending |
| FRAUD-02 | P1: Endpoint `/fraud-score` (decisão correta) | Design | Pending |
| FRAUD-03 | P1: Endpoint `/fraud-score` (`-1` em null) | Design | Pending |
| FRAUD-04 | P1: Endpoint `/fraud-score` (p99 ≤ 1 ms, falhas < 1%) | Design | Pending |
| FRAUD-05 | P1: Endpoint `/fraud-score` (mcc default 0.5) | Design | Pending |
| READY-01 | P1: `/ready` 200 quando pronto | Design | Pending |
| READY-02 | P1: `/ready` não-200 antes do índice | Design | Pending |
| TOPO-01 | P1: soma cpus ≤ 1.00 | Design | Pending |
| TOPO-02 | P1: soma memory ≤ 350 MB | Design | Pending |
| TOPO-03 | P1: round-robin sem lógica de negócio | Design | Pending |
| TOPO-04 | P1: bridge / sem privileged | Design | Pending |
| BUILD-01 | P2: build determinístico do índice | Design | Pending |
| BUILD-02 | P2: cache invalidation do índice | - | Pending |
| EDGE-01 | Edge: payload inválido | Design | Pending |
| EDGE-02 | Edge: MCC desconhecido | Design | Pending |
| EDGE-03 | Edge: known_merchants vazio | Design | Pending |
| EDGE-04 | Edge: clamp amount/installments | Design | Pending |
| EDGE-05 | Edge: zona ambígua nprobe | Design | Pending |
| EDGE-06 | Edge: índice ausente | Design | Pending |

**ID format:** `[CATEGORY]-[NUMBER]` — categorias: `FRAUD`, `READY`, `TOPO`, `BUILD`, `EDGE`.
**Status values:** Pending → In Design → In Tasks → Implementing → Verified.
**Coverage:** 19 totais, 0 mapeados a tarefas (será atualizado em `tasks.md`).

---

## Success Criteria

- [ ] `results.json` da execução oficial mostra `final_score ≥ +5500`.
- [ ] `p99` reportado pelo k6 ≤ `1 ms` em pelo menos 3 execuções consecutivas locais.
- [ ] `failure_rate < 1%` em pelo menos 3 execuções consecutivas locais.
- [ ] Soma de `cpus` no compose = `1.00`, soma de `memory` ≤ `350 MB`.
- [ ] `docker compose up --build` em clone limpo do repo sobe o stack e `/ready` responde 200 em ≤ 3 s.
