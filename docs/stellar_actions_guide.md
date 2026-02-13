# NeuroChain Soroban — Stellar Actions Guide

Tämä on **elävä käyttöohje** NeuroChain‑Sorobanin CLI‑polulle. Päivitetään tätä dokumenttia aina, kun uusia actioneita, preview‑tietoja tai guardrailseja lisätään.

## Mikä tämä on?

`neurochain-soroban` lukee `.nc`‑tiedoston ja muuntaa rivit **ActionPlan**‑JSONiksi. Kun käytät `--flow`, se ajaa polun:

**simulate → preview → confirm → submit**

MVP‑vaiheessa tuettuja toimintoja:

- **FundTestnet** (Friendbot)
- **BalanceQuery** (Horizon)
- **CreateAccount** (stellar CLI)
- **ChangeTrust** (stellar CLI)
- **Payment** (stellar CLI, XLM + issued assets)
- **TxStatus** (Horizon)
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
- `NC_TXREP_PREVIEW=1`
  - lisää txrep/SEP‑11‑previewn (ihmisluettava XDR)
  - jos CLI ei tue `tx to-rep`, fallback `tx decode` (json‑formatted)

**IntentStellar mode:**

- `NC_INTENT_STELLAR_MODEL`
  - intent_stellar ONNX-polku (oletus: `models/intent_stellar/model.onnx`)
- `NC_INTENT_STELLAR_THRESHOLD`
  - confidence-kynnys (oletus: `0.55`)

**Allowlist (valinnainen, mutta suositus):**

- `NC_ASSET_ALLOWLIST` (esim. `XLM,USDC:GISSUER`)
- `NC_SOROBAN_ALLOWLIST` (esim. `C1:transfer,C2`)
- `NC_ALLOWLIST_ENFORCE=1` → hard‑fail (muuten vain varoitus)

Testnet‑USDC esimerkki (Stellar Expert):

```powershell
setx NC_ASSET_ALLOWLIST "XLM,USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
```

---

## 3) Käyttö — pelkkä JSON‑ActionPlan

```powershell
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc
```

Tulos on ActionPlan JSON, joka kertoo mitä **aikoisi** tehdä.

## 3.5) Käyttö — `--intent-text` (IntentStellar -> ActionPlan)

```powershell
cargo run --bin neurochain-soroban -- --intent-text "Transfer 5 XLM to G..."
```

Mallin polku/kynnys voidaan overrideata:

```powershell
cargo run --bin neurochain-soroban -- --intent-text "Transfer 5 XLM to G..." --intent-model models\intent_stellar\model.onnx --intent-threshold 0.60
```

Turvablockki:
- jos intent on low-confidence tai slotit puuttuvat, ActionPlaniin tulee `unknown` + `intent_error`/`intent_warning`
- `--flow`-tilassa submit skipataan turvallisesti ja prosessi palauttaa exit-koodin `5`

## 3.6) Käyttö — interactive REPL (`AI:` + promptit)

```powershell
cargo run --bin neurochain-soroban
```

REPL-komennot:
- `AI: "models/intent_stellar/model.onnx"` vaihtaa intent-mallin
- `network: testnet` (tai `set network = "testnet"`) vaihtaa aktiivisen verkon
- `wallet: nc-testnet` (tai `set wallet = "nc-testnet"`) vaihtaa lähdelompakon (source alias)
- `set intent from AI: "Transfer 5 XLM to G..."` ajaa intent -> ActionPlan
- `macro from AI: "Transfer 5 XLM to G..."` toimii aliasina prompt-ajolle
- `stellar.*` / `soroban.*` rivit toimivat manuaalisena action-plan syötteenä
- `help`, `exit`

## 3.7) Käyttö — `.nc` scripti samoilla komennoilla

Samat meta-rivit toimivat nyt myös tiedostossa (`neurochain-soroban script.nc`):

```nc
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
set intent from AI: "Transfer 5 XLM to G..."
```

```powershell
cargo run --bin neurochain-soroban -- examples\intent_stellar_smoke.nc --flow
```

---

## 4) Käyttö — simulate → preview → confirm → submit

```powershell
cargo run --bin neurochain-soroban -- examples\stellar_actions_example.nc --flow
```

- Preview näyttää **fee‑arvion** (Horizon `fee_stats`) ja **efektit**.
- `--yes` ohittaa vahvistuskyselyn.
- Submit‑tulosteet näyttävät **tx‑hashin**, jos se voidaan päätellä.  
  Jos CLI‑outputista ei löydy hashia, haetaan viimeisin tx‑hash Horizonista
  ja merkitään `(latest)`.
- Submit‑rivit ovat nyt yhtenäisessä muodossa: `status=ok|error`, `tx_hash`, `return`.
- Soroban‑simuloinnissa tyhjä output tulkitaan “ok”‑tulokseksi.
- Jos `NC_TXREP_PREVIEW=1`, preview tulostaa txrep‑muodon jokaisesta actionista.
  Jos `to-rep` ei ole saatavilla, tulostetaan `tx decode` ‑JSON.

## 4.5) Contract‑policy (schema‑guardrail)

Soroban‑invoke voidaan validoida contract‑kohtaisella policyllä ennen simulate‑polkua.

- Policy‑tiedosto: `contracts/<name>/policy.json` (tai suora polku `NC_CONTRACT_POLICY`)
- Enforce: `NC_CONTRACT_POLICY_ENFORCE=1` → hard‑fail

Tuetut arg‑tyypit:
- `string | number | bool | address | symbol | bytes`  
  - `address`: strkey (G… / C…, 56 merkkiä)  
  - `symbol`: 1–32 ASCII, ei whitespace  
  - `bytes`: hex muodossa `0x...`

**Esimerkki (hello‑contract):**
```json
{
  "contract_id": "C...",
  "allowed_functions": ["hello"],
  "args_schema": {
    "hello": {
      "required": { "to": "symbol" },
      "optional": {}
    }
  },
  "max_fee_stroops": 1000
}
```

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

**Huom:** määrät (`amount`, `starting_balance`, `limit`) tulkitaan XLM‑tyylisinä desimaaleina ja muunnetaan stroopeiksi (7 desimaalia) ennen submitia.

---

## 6) USDC‑flow (erillinen esimerkki)

Repoon on lisätty **valmis TESTUSD‑flow** (issuer‑omistettu test‑asset):

```
examples/stellar_testasset_trustline.nc
examples/stellar_testasset_issue.nc
examples/stellar_testasset_payment.nc
examples/stellar_testasset_user_trustline.nc
examples/stellar_testasset_user_payment.nc
```

Tämä on **kahden vaiheen** ajettava:

1) **Receiver** tekee trustlinen  
   Aseta `NC_SOROBAN_SOURCE=<receiver-alias>` ja pidä **vain change_trust** rivi aktiivisena.

2) **Sender** tekee USDC‑paymentin  
   Aseta `NC_SOROBAN_SOURCE=<sender-alias>` ja pidä **vain USDC payment** rivi aktiivisena.

Vaihtoehtona voit käyttää erillisiä tiedostoja (ei kommentointia):

- `examples/stellar_usdc_trustline.nc` → receiver
- `examples/stellar_usdc_payment.nc` → sender

**Käyttökomennot:**

```powershell
# Receiver (trustline)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-soroban -- examples\stellar_usdc_trustline.nc --flow

# Sender (USDC payment)
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-soroban -- examples\stellar_usdc_payment.nc --flow
```

**Test‑asset (oma issuer) – 3 askelta:**

```powershell
# 1) Receiver trustline (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-soroban -- examples\stellar_testasset_trustline.nc --flow

# 2) Issuer issues TESTUSD to receiver (nc-testnet)
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-soroban -- examples\stellar_testasset_issue.nc --flow

# 3) Receiver sends TESTUSD back (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-soroban -- examples\stellar_testasset_payment.nc --flow
```

**3‑tilin malli (distributor → user):**

Korvaa `GUSER...` oikealla käyttäjä‑tilillä ja aja:

```powershell
# User trustline (user alias)
$env:NC_SOROBAN_SOURCE="user-alias"
cargo run --bin neurochain-soroban -- examples\stellar_testasset_user_trustline.nc --flow

# Distributor -> user payment (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-soroban -- examples\stellar_testasset_user_payment.nc --flow
```

---

## 7) Soroban invoke vaatii CLI‑avaimen

Soroban invoke käyttää `stellar contract invoke`‑komentoa. Aseta **alias**:

```powershell
# esimerkki: aseta key alias "quest1-new" ja käytä sitä
setx NC_SOROBAN_SOURCE "quest1-new"
```

---

## 8) Txrep‑muunnokset (ActionPlan + JSONL)

NeuroChainissa on kaksi **txrep‑muunnostyökalua**:

- `txrep-to-action` → muuntaa `stellar tx decode --output json-formatted` ‑datan **ActionPlan**‑muotoon.
- `txrep-to-jsonl` → muuntaa saman txrep‑datan **JSONL**‑rivimuotoon (dataset‑pipelinea varten).

**Esimerkki:**

```powershell
# 1) Dekoodaa XDR → txrep (json-formatted)
stellar tx decode --input <TX_XDR_BASE64> --output json-formatted > txrep.json

# 2) Txrep → ActionPlan
cargo run --bin txrep-to-action -- txrep.json > action_plan.json

# 3) Txrep → JSONL (dataset‑pipeline)
cargo run --bin txrep-to-jsonl -- txrep.json > dataset.jsonl
```

> Huom: nämä eivät tee on‑chain‑kutsuja — ne ovat puhtaita **muunnostyökaluja**.

---

## 9) Yleisimmät virheet

- **Friendbot error** → varmista testnet + public key
- **Horizon 404** → tili ei ole vielä luotu/rahoitettu
- **Soroban invoke failed** → contract_id / function / allowlist / CLI key

---

## 10) Seuraavaksi (roadmap)

- Soroban invoke output‑parsinta (fee/preview erittely)
- (Valinnainen) **txrep / SEP‑11** preview‑tulostus audit‑trailiin (ihmisluettava XDR)

---

## 11) Päivitysperiaate

Tätä ohjetta päivitetään aina, kun:

- uusia actioneita lisätään
- previewn sisältö laajenee
- guardrails‑logiikka muuttuu
