# NeuroChain Stellar — Stellar Actions Guide

Tämä on **elävä käyttöohje** NeuroChain‑Stellarin CLI‑polulle. Päivitetään tätä dokumenttia aina, kun uusia actioneita, preview‑tietoja tai guardrailseja lisätään.

## Mikä tämä on?

`neurochain-stellar` lukee `.nc`‑tiedoston ja muuntaa rivit **ActionPlan**‑JSONiksi. Kun käytät `--flow`, se ajaa polun:

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

### CLI ajosyntaksi (Cargo vs binäärin argumentit)

Tärkeä sääntö: `cargo run` tarvitsee erotinmerkin `--`, jotta argumentit menevät **neurochain-stellar**-binäärille eikä Cargolle.

```powershell
# OIKEIN: REPL (flow oletuksena päällä)
cargo run --release --bin neurochain-stellar

# OIKEIN: REPL plan-only (ei simulate/submit)
cargo run --release --bin neurochain-stellar -- --no-flow

# OIKEIN: explicit flow (valinnainen, ei pakollinen REPL:ssä)
cargo run --release --bin neurochain-stellar -- --flow

# VÄÄRIN: --flow menee Cargolle -> "unexpected argument '--flow'"
cargo run --release --bin neurochain-stellar --flow
```

Huom:
- `cargo run --bin neurochain-stellar ...` ilman `--release` = **DEBUG/DEV-tila** (`target\debug\...`).
- `cargo run --release --bin neurochain-stellar ...` = **RELEASE-tila** (`target\release\...`), optimoitu ajo.

### Pääajot (suositus, RELEASE)

```powershell
cd <project-root>

# 1) Normaali CLI/REPL ajo
cargo run --release --bin neurochain-stellar

# 2) Plan-only REPL (jos et halua simulate/submit tässä sessiossa)
cargo run --release --bin neurochain-stellar -- --no-flow
```

Nämä kaksi ovat pääkomennot päivittäiseen käyttöön.
Huom: normaali REPL ajaa flow-putkea oletuksena (`simulate -> preview -> confirm -> submit`).

### Debug-ajot (DEV/DEBUG)

```powershell
# Normaali CLI/REPL ajo (debug)
cargo run --bin neurochain-stellar

# Plan-only REPL (debug)
cargo run --bin neurochain-stellar -- --no-flow
```

### Muut ajotavat (tarvittaessa, RELEASE)

```powershell
# Intent prompt suoraan CLI-flagilla
cargo run --release --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..."

# .nc tiedosto
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_payment_flow.nc

# .nc flow (simulate -> preview -> confirm -> submit)
cargo run --release --bin neurochain-stellar -- examples\intent_stellar_payment_flow.nc --flow
```

Tarvittaessa samat komennot debugilla: poista `--release`.

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

**Contract policy (valinnainen, mutta suositus):**

- `NC_CONTRACT_POLICY` (suora policy.json-polku)
- `NC_CONTRACT_POLICY_DIR` (policy-hakemisto, oletus: `contracts`)
- `NC_CONTRACT_POLICY_ENFORCE=1` → hard‑fail (muuten vain varoitus)

### 2.1) Env-matriisi: mitä tekee missäkin

Samat envit pätevät sekä CLI-ajossa (`--intent-text` / tiedostoajo) että `.nc` script-ajossa.
REPLissä samat asiat voi asettaa envin sijaan myös riveillä (`network: ...`, `wallet: ...`, `txrep`, jne.).

| Env | Mitä tekee | Vaikuttaa (CLI/REPL/.nc) | Oletus |
|---|---|---|---|
| `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK` | Asettaa verkon | CLI + REPL + `.nc` | `testnet` |
| `NC_STELLAR_HORIZON_URL` | Asettaa Horizon-URL:n | CLI + REPL + `.nc` | verkosta johdettu |
| `NC_FRIENDBOT_URL` | Asettaa Friendbot-URL:n | CLI + REPL + `.nc` | testnet friendbot |
| `NC_SOROBAN_SOURCE` / `NC_STELLAR_SOURCE` | Asettaa source-lompakon aliasin | CLI + `.nc` (REPL wallet asetetaan eksplisiittisesti) | ei asetettu |
| `NC_STELLAR_CLI` | Asettaa käytettävän `stellar`-binäärin | CLI + REPL + `.nc` | `stellar` |
| `NC_SOROBAN_SIMULATE_FLAG` | Asettaa simulate-flagin | CLI + REPL + `.nc` | `--send no` |
| `NC_TXREP_PREVIEW` | Kytkee txrep-previewn päälle | CLI + REPL + `.nc` | off |
| `NC_INTENT_STELLAR_MODEL` | Asettaa intent_stellar-mallipolun | CLI + REPL + `.nc` | `models/intent_stellar/model.onnx` |
| `NC_INTENT_STELLAR_THRESHOLD` | Asettaa intent confidence-kynnyksen | CLI + REPL + `.nc` | `0.55` |
| `NC_ASSET_ALLOWLIST` | Asettaa asset-allowlistin | CLI + REPL + `.nc` | tyhjä |
| `NC_SOROBAN_ALLOWLIST` | Asettaa contract/function-allowlistin | CLI + REPL + `.nc` | tyhjä |
| `NC_ALLOWLIST_ENFORCE` | Kytkee allowlistin hard-failiksi | CLI + REPL + `.nc` | off (warning-only) |
| `NC_CONTRACT_POLICY` | Yksittäinen policy.json-polku | CLI + REPL + `.nc` | ei asetettu |
| `NC_CONTRACT_POLICY_DIR` | Policy-hakemisto | CLI + REPL + `.nc` | `contracts` |
| `NC_CONTRACT_POLICY_ENFORCE` | Kytkee policy-rikkeet hard-failiksi | CLI + REPL + `.nc` | off (warning-only) |

### 2.2) Sama ilman env-muuttujia (REPL / `.nc`)

Voit asettaa samat arvot suoraan CLI:n sisällä (REPL) tai `.nc`-scriptissä:
Suositus: laita nämä **heti alkuun** (AI -> network -> wallet -> muut asetukset -> intent/action).

- `NC_STELLAR_NETWORK` / `NC_SOROBAN_NETWORK` -> `network: testnet`
- `NC_STELLAR_HORIZON_URL` -> `horizon: https://horizon-testnet.stellar.org`
- `NC_FRIENDBOT_URL` -> `friendbot: https://friendbot.stellar.org` tai `friendbot: off`
- `NC_SOROBAN_SOURCE` / `NC_STELLAR_SOURCE` -> `wallet: nc-testnet` (tai `source: nc-testnet`)
- `NC_STELLAR_CLI` -> `stellar_cli: stellar`
- `NC_SOROBAN_SIMULATE_FLAG` -> `simulate_flag: "--send no"`
- `NC_TXREP_PREVIEW=1` -> `txrep` / `txrep on` (`txrep off` pois päältä)
- `NC_INTENT_STELLAR_MODEL` -> `AI: "models/intent_stellar/model.onnx"`
- `NC_INTENT_STELLAR_THRESHOLD` -> `intent_threshold: 0.55`
- `NC_ASSET_ALLOWLIST` -> `asset_allowlist: XLM,USDC:GISSUER`
- `NC_SOROBAN_ALLOWLIST` -> `soroban_allowlist: C1:transfer,C2`
- `NC_ALLOWLIST_ENFORCE` -> `allowlist_enforce` (päälle) / `allowlist_enforce off` (pois)

Esimerkki (REPL tai `.nc`):

```nc
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
txrep
asset_allowlist: XLM
allowlist_enforce
set stellar intent from AI: "Transfer 5 XLM to G..."
```

Testnet‑USDC esimerkki (Stellar Expert):

```powershell
setx NC_ASSET_ALLOWLIST "XLM,USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
```

### 2.5) Enforce behavior + exit‑koodit

Validoinnit ajetaan aina, mutta blokkauksen taso riippuu enforce‑envistä:

- `NC_ALLOWLIST_ENFORCE=0` (tai unset): allowlist-rikkeet = warning, ajo jatkuu.
- `NC_ALLOWLIST_ENFORCE=1`: allowlist-rike = hard-fail, prosessi poistuu koodilla `3`.
- `NC_CONTRACT_POLICY_ENFORCE=0` (tai unset): policy-rikkeet = warning, ajo jatkuu.
- `NC_CONTRACT_POLICY_ENFORCE=1`: policy-rike = hard-fail, prosessi poistuu koodilla `4`.
- Intent-tilassa (`--intent-text` tai `set stellar intent from AI`) `Unknown` / `intent_error` / `intent_warning`
  blokkkaa flown turvallisesti ja palauttaa koodin `5`.

---

## 3) Käyttö — pelkkä JSON‑ActionPlan

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc
```

Tulos on ActionPlan JSON, joka kertoo mitä **aikoisi** tehdä.

## 3.5) Käyttö — `--intent-text` (IntentStellar -> ActionPlan)

```powershell
cargo run --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..."
```

Mallin polku/kynnys voidaan overrideata:

```powershell
cargo run --bin neurochain-stellar -- --intent-text "Transfer 5 XLM to G..." --intent-model models\intent_stellar\model.onnx --intent-threshold 0.60
```

Turvablockki:
- jos intent on low-confidence tai slotit puuttuvat, ActionPlaniin tulee `unknown` + `intent_error`/`intent_warning`
- `--flow`-tilassa submit skipataan turvallisesti ja prosessi palauttaa exit-koodin `5`

## 3.6) Käyttö — interactive REPL (`AI:` + promptit)

```powershell
cargo run --bin neurochain-stellar
```

Huom (wallet-startup REPLissä):
- REPL käynnistyy aina tilaan `Current wallet/source: (not set)`.
- Tämä on tarkoituksella wallet-explicit UX: aseta lompakko itse komennolla `wallet: <alias>` (tai `source: <alias>`).
- `setup testnet` ei aseta walletia automaattisesti.

REPL-komennot (`help all`) jaoteltuna:

Core setup (value required):
- `AI: "path"` -> set intent model path
- `intent_threshold: <f32>` -> set intent confidence threshold
- `network: testnet|mainnet|public` -> set active network for flow
- `wallet: <stellar-key-alias>` -> set active source wallet alias
- `horizon: https://...` -> set Horizon URL override
- `friendbot: https://...|off` -> set Friendbot URL or disable it
- `stellar_cli: <bin>` -> set stellar CLI binary path/name
- `simulate_flag: "--send no"` -> set soroban simulate flag
- `asset_allowlist: XLM,USDC:G...` -> set NC_ASSET_ALLOWLIST equivalent
- `soroban_allowlist: C1:transfer,C2` -> set NC_SOROBAN_ALLOWLIST equivalent

Toggles (on/off):
- `txrep` -> enable txrep preview in flow
- `txrep off` -> disable txrep preview in flow
- `allowlist_enforce` -> enable allowlist enforce
- `allowlist_enforce off` -> disable allowlist enforce

Prompt/Action commands:
- `set <var> from AI: "..."` -> predict with active model and store variable
- `set stellar intent from AI: "Transfer 5 XLM to G..."` -> classify prompt -> ActionPlan
- `set intent from AI: "Transfer 5 XLM to G..."` -> legacy alias (still supported)
- `macro from AI: "..."` -> not supported in `neurochain-stellar` (use `set stellar intent from AI`)
- `plain text prompt` -> classify prompt -> ActionPlan
- `stellar.* / soroban.* lines` -> manual action-plan mode

Utility commands:
- `help` -> quick start
- `help all` -> show every command
- `help dsl` -> show normal NeuroChain DSL language help
- `show setup` -> print active setup
- `show config` -> print active config
- `setup testnet` -> set network+horizon+friendbot baseline
- `exit` -> leave REPL

Yhtenäinen toggle-sääntö:
- Pelkkä asetusrivi kytkee päälle (`txrep`, `allowlist_enforce`)
- `off` samassa rivissä kytkee pois (`txrep off`, `allowlist_enforce off`)

## 3.7) Käyttö — `.nc` scripti samoilla komennoilla

Samat meta-rivit toimivat nyt myös tiedostossa (`neurochain-stellar script.nc`):

```nc
AI: "models/intent_stellar/model.onnx"
network: testnet
wallet: nc-testnet
txrep
set stellar intent from AI: "Transfer 5 XLM to G..."
```

```powershell
cargo run --bin neurochain-stellar -- examples\intent_stellar_smoke.nc --flow
```

Huom:
- `.nc` script-ajossa CLI tulostaa ennen suoritusta `Script execution setup` -yhteenvedon (stderr),
  jotta näet käytössä olevat asetukset (`network`, `wallet/source`, `flow_mode`, `txrep_preview`, allowlistit).
- `.nc` script-ajo noudattaa samoja sääntöjä kuin CLI/REPL:
  - sama validointi (`validate_plan`)
  - sama allowlist/policy-enforce-käytös
  - sama flow-käytös (`--flow` vs plan-only)
  - samat intent safety block -säännöt ja exit-koodit.

### 3.7.1) Monimallinen `if`-putki samassa `.nc`-ajossa

Scriptissä voi käyttää useita malleja yhdessä ajossa:

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "This is wonderful!"
if mood == "Positive":
    AI: "models/intent_stellar/model.onnx"
    set stellar intent from AI: "Transfer 5 XLM to G..."
```

Valmis esimerkki: `examples/multi_model_if_payment.nc`

## 3.8) `--flow` vs ilman `--flow` (tärkeä)

- REPLissä (`cargo run --bin neurochain-stellar`) flow on oletuksena päällä.
- `--no-flow` pakottaa REPLin plan-only-tilaan (ei simulaatiota/submitia).
- Tiedosto-/`--intent-text`-ajossa ilman `--flow`: tulostetaan vain `ActionPlan` JSON (dry-run).
- Tiedosto-/`--intent-text`-ajossa `--flow` kanssa: ajetaan `simulate -> preview -> confirm -> submit`.
- `Y/N`-vahvistus näkyy vain flow-tilassa (`Confirm submit? [y/N]`).
- `--yes` ohittaa vahvistuskyselyn flow-tilassa.
- `NC_TXREP_PREVIEW=1` vaikuttaa preview-vaiheeseen, joten se näkyy käytännössä flow-ajossa.

Nopea yhteenveto:
- `cargo run --bin neurochain-stellar` = REPL, flow oletuksena päällä.
- `cargo run --bin neurochain-stellar -- --no-flow` = REPL plan-only.
- `cargo run --bin neurochain-stellar -- <input>` = tiedosto/intent dry-run.
- `cargo run --bin neurochain-stellar -- <input> --flow` = tiedosto/intent voi tehdä oikean submitin.

---

## 4) Käyttö — simulate → preview → confirm → submit

```powershell
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc --flow
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
cargo run --bin neurochain-stellar -- examples\stellar_actions_example.nc --flow --yes
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
cargo run --bin neurochain-stellar -- examples\stellar_usdc_trustline.nc --flow

# Sender (USDC payment)
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-stellar -- examples\stellar_usdc_payment.nc --flow
```

**Test‑asset (oma issuer) – 3 askelta:**

```powershell
# 1) Receiver trustline (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_trustline.nc --flow

# 2) Issuer issues TESTUSD to receiver (nc-testnet)
$env:NC_SOROBAN_SOURCE="nc-testnet"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_issue.nc --flow

# 3) Receiver sends TESTUSD back (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_payment.nc --flow
```

**3‑tilin malli (distributor → user):**

Korvaa `GUSER...` oikealla käyttäjä‑tilillä ja aja:

```powershell
# User trustline (user alias)
$env:NC_SOROBAN_SOURCE="user-alias"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_user_trustline.nc --flow

# Distributor -> user payment (nc-new)
$env:NC_SOROBAN_SOURCE="nc-new"
cargo run --bin neurochain-stellar -- examples\stellar_testasset_user_payment.nc --flow
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
