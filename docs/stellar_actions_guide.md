# NeuroChain Soroban — Stellar Actions Guide

Tämä on **elävä käyttöohje** NeuroChain‑Sorobanin CLI‑polulle. Päivitetään tätä dokumenttia aina, kun uusia actioneita, preview‑tietoja tai guardrailseja lisätään.

## Mikä tämä on?

`neurochain-soroban` lukee `.nc`‑tiedoston ja muuntaa rivit **ActionPlan**‑JSONiksi. Kun käytät `--flow`, se ajaa polun:

**simulate → preview → confirm → submit**

MVP‑vaiheessa tuettuja toimintoja:

- **FundTestnet** (Friendbot)
- **BalanceQuery** (Horizon)
- **Soroban invoke** (stellar CLI)

Muut Classic‑actionit (payment/change_trust/create_account) ovat seuraavassa vaiheessa.

---

## 1) Asennus & perusvalmistelut

### Varmista nämä työkalut

- Rust + Cargo
- `stellar` CLI (Soroban)

### Projektin ajo

```powershell
cd C:\Users\Ville\Desktop\neurochain_dsl_soroban
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc
```

---

## 2) Ympäristömuuttujat (MVP)

**Verkko & API:**

- `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK` (default: `testnet`)
- `NC_STELLAR_HORIZON_URL` (default: testnet Horizon)
- `NC_FRIENDBOT_URL` (vain testnet, default: friendbot)

**Soroban invoke:**

- `NC_SOROBAN_SOURCE` tai `NC_STELLAR_SOURCE`
  - Stellar‑CLI key alias (ei secret‑key suoraan)
- `NC_STELLAR_CLI`
  - jos `stellar` ei ole PATHissa
- `NC_SOROBAN_SIMULATE_FLAG`
  - oletus: `--send no` (CLI 25+), esim. `--send no` tai `--send=no`

**Allowlist (valinnainen, mutta suositus):**

- `NC_ASSET_ALLOWLIST` (esim. `XLM,USDC:GISSUER`)
- `NC_SOROBAN_ALLOWLIST` (esim. `C1:transfer,C2`)
- `NC_ALLOWLIST_ENFORCE=1` → hard‑fail (muuten vain varoitus)

---

## 3) Käyttö — pelkkä JSON‑ActionPlan

```powershell
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc
```

Tulos on ActionPlan JSON, joka kertoo mitä **aikoisi** tehdä.

---

## 4) Käyttö — simulate → preview → confirm → submit

```powershell
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc --flow
```

- Preview näyttää **fee‑arvion** (Horizon `fee_stats`) ja **efektit**.
- `--yes` ohittaa vahvistuskyselyn.

```powershell
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc --flow --yes
```

---

## 5) `.nc`‑rivit (MVP‑syntax)

Rivit alkavat `stellar.` tai `soroban.`. Inline‑kommentit `#`/`//` sallittu.  
**Kommenttirivit** (rivit, jotka alkavat `#` tai `//`) ohitetaan kokonaan.

```nc
# BalanceQuery
stellar.account.balance account="G..." asset="XLM"

# Fund testnet
stellar.account.fund_testnet account="G..."

# Soroban invoke
soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"G...","amount":100}
```

---

## 6) Soroban invoke vaatii CLI‑avaimen

Soroban invoke käyttää `stellar contract invoke`‑komentoa. Aseta **alias**:

```powershell
# esimerkki: aseta key alias "quest1-new" ja käytä sitä
setx NC_SOROBAN_SOURCE "quest1-new"
```

---

## 7) Yleisimmät virheet

- **Friendbot error** → varmista testnet + public key
- **Horizon 404** → tili ei ole vielä luotu/rahoitettu
- **Soroban invoke failed** → contract_id / function / allowlist / CLI key

---

## 8) Seuraavaksi (roadmap)

- `stellar.payment` (XLM) end‑to‑end
- `stellar.change_trust`
- `stellar.account.create`
- Soroban invoke output‑parsinta (fee/preview erittely)

---

## 9) Päivitysperiaate

Tätä ohjetta päivitetään aina, kun:

- uusia actioneita lisätään
- previewn sisältö laajenee
- guardrails‑logiikka muuttuu
