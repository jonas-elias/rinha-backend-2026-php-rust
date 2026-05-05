# State — Rinha 2026 PHP+Rust

**Last updated:** sessão de planejamento inicial (TLC SDD).

## Decisions

- **D1 — Bridge PHP↔Rust:** extensão Zend via `ext-php-rs` (cdylib), em vez de `php-ffi`. Motivo: overhead da chamada na ordem de dezenas de ns vs. ~1–3 µs do FFI; p99=1 ms exige cada microssegundo.
- **D2 — Load balancer:** HAProxy 2.9 em `mode tcp` com `balance roundrobin` e upstream UDS. Imagem oficial `haproxy:2.9-alpine` é pública e leve. Reaproveitar `jrblatt/so-no-forevis` foi descartado por opacidade.
- **D3 — Algoritmo:** IVF (k-means k=4096, AVX2 + FMA) com vetores quantizados i16, espelhando o 2º colocado (rinha-2026-rust). HNSW descartado por complexidade vs. ganho marginal a 3M.
- **D4 — Dataset:** começar com os 3M vetores completos. Subamostrar (200–500 k estratificado) só se o tuning falhar em convergir.
- **D5 — Comunicação LB↔API:** Unix Domain Sockets em volume tmpfs. TCP loopback descartado (~40–60 µs/req extras observados nos vencedores).
- **D6 — Containers:** PHP em `php:8.3-cli-bookworm-slim` (glibc) — incompatibilidade conhecida do `ext-php-rs` com musl alpine no momento. Custo ~10 MB extras é aceitável.
- **D7 — Swoole:** `SWOOLE_BASE`, `worker_num=1`, `reactor_num=1`, listener UDS. Cada API tem 1 worker; paralelismo vem das 2 APIs balanceadas pelo LB.

## Blockers

- *(nenhum no momento)*

## Lessons / observações

- A leitura do código dos dois vencedores convergiu na mesma receita: UDS+tmpfs, IVF+AVX2, JSON manual, respostas estáticas. Qualquer desvio significativo dessa receita precisa de justificativa forte.
- O 2º colocado subamostrou para 100 k vetores e ainda assim ficou no top 2 — sinal de que IVF reduz a sensibilidade a `n` e que qualidade de detecção é dominada pelo algoritmo, não pelo tamanho do dataset.

## Todos (não-feature)

- Confirmar nome final do diretório (`rinha-2026-php-rust` está adotado por enquanto).

## Deferred ideas

- **Publicação da imagem em registry público** + branch `submission` + PR em `participants/` + issue `rinha/test` (esperar MVP estabilizar antes).
- Escrever `ARTICLE.md` no estilo do 1º colocado depois da submissão.
- Avaliar HNSW se IVF travar em algum tradeoff insatisfatório.
- Avaliar mover loop de `accept()` para Rust e expor `run_server()` à PHP — só se Swoole virar gargalo.

## User preferences (capturadas nesta sessão)

- Versão de PHP **flutuante** na tag `8.3-cli-bookworm-slim` (sem pin de patch).
- `references.json.gz` pode ser copiado para o build context **desde que não afete a performance final** — usar apenas no stage `index-builder` e descartar depois (multi-stage isola a cópia, então zero impacto runtime).
- Não se preocupar agora com geração/publicação de imagem Docker em registry; foco no MVP funcional + tuning local.
