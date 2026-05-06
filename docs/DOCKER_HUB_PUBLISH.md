# Publicação no Docker Hub via GitHub Actions

O **build e o push acontecem no runner do GitHub**, não na sua máquina. Você só precisa cadastrar 1 variável e 1 secret no repo. Depois disso, todo `push` em `main` (ou `workflow_dispatch` manual) republica a imagem **e roda um smoke contra a imagem publicada** — essa é a sua garantia de que ela funciona.

## 1. Gere um Personal Access Token no Docker Hub

Você só precisa fazer isso 1 vez, do **celular ou de uma rede pessoal** (não da
empresa):

1. Acesse https://hub.docker.com/settings/security (logue com a conta vinculada
   ao gmail `joxxxdasilvaexxxx@gmail.com`).
2. Clique em **New Access Token**:
   - **Description**: `gh-actions rinha-2026-php-rust`
   - **Permissions**: `Read & Write` (precisa pra criar o repo público e dar push)
3. Copie o token (formato `dckr_pat_...`). Ele só aparece 1 vez.
4. Anote também o seu **username** no Docker Hub (canto superior direito do site
   — não é o e-mail, é o handle público).

## 2. Cadastre no repo (1 comando, do seu shell)

Substitua `SEU_USERNAME` e `SEU_TOKEN` pelos valores do passo anterior:

```bash
gh variable set DOCKERHUB_USERNAME \
  -R jonas-elias/rinha-backend-2026-php-rust \
  --body 'SEU_USERNAME'

gh secret set DOCKERHUB_TOKEN \
  -R jonas-elias/rinha-backend-2026-php-rust \
  --body 'SEU_TOKEN'
```

(Alternativa via UI: repo → Settings → Secrets and variables → Actions.
 *Variables* recebe `DOCKERHUB_USERNAME`, *Secrets* recebe `DOCKERHUB_TOKEN`.)

## 3. Dispare o workflow

```bash
gh workflow run "build & push runtime image" \
  -R jonas-elias/rinha-backend-2026-php-rust
gh run watch -R jonas-elias/rinha-backend-2026-php-rust
```

## 4. O que o workflow faz (e por que é a sua garantia)

Ele tem 2 jobs em sequência:

1. **build-push** (job `ubuntu-latest`, ~25–35 min):
   - Baixa `references.json.gz` do repo upstream da Rinha.
   - Constrói a imagem (k-means + LTO=fat).
   - Faz `docker login` no Docker Hub usando `DOCKERHUB_TOKEN`.
   - Dá `docker push` em 2 tags: `latest` e o SHA curto do commit.
2. **verify** (depende de `build-push`, ~3 min):
   - Faz `docker pull SEU_USERNAME/rinha-2026-php-rust:<sha>` —
     **isso prova que a imagem realmente está no Docker Hub público**.
   - Sobe a imagem e faz `curl` em `/ready` (espera 200) e `POST /fraud-score`
     (espera JSON com `approved` e `fraud_score`) via UNIX socket.
   - Imprime um summary com o digest e o comando de pull pra você usar local.

Se o passo `verify` passar, você tem 3 evidências:

- ✅ Push deu certo (senão o login falharia)
- ✅ A imagem é pullável publicamente (senão o pull no job seguinte falha)
- ✅ A imagem inicia e atende as rotas certas (smoke ao vivo)

## 5. Como verificar de outra máquina (qualquer lugar sem login)

Pull público não exige autenticação:

```bash
docker pull SEU_USERNAME/rinha-2026-php-rust:latest
```
