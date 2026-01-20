# Stellar MVP (NeuroChain DSL → Classic Stellar + Soroban)

This document is a **step-by-step execution plan** for the Stellar/Soroban MVP.
It distills the core ideas from `docs/soroban_integration.md` into a concrete build order.

## MVP goal (1 sentence)

Turn user intent into a deterministic **action plan**, and run it safely through a **simulate → preview → confirm → submit** flow, via either the CLI or an Axum API.

## Principles (non-negotiable)

- **Determinism:** ONNX/intent only classifies; execution logic is deterministic.
- **Simulate-first:** never submit on-chain before simulation + preview.
- **Guardrails:** confidence threshold + allowlist + limits + explicit confirm.
- **No secrets in logs:** never log secret keys, seed phrases, signatures, or XDR.
- **Two runtimes:** one shared core, two frontends (CLI + Axum).

## MVP scope

The MVP covers two “buckets” of functionality:

- **Classic Stellar (account + assets)**
  - Account: create / fund_testnet / balance
  - Asset: change_trust
  - Payment: XLM + issued assets
  - Status: tx status

- **Soroban Smart (contracts)**
  - Contract invoke (allowlisted)

Passkeys, smart wallet policies, sponsorship/fee-bumps, TTL/rent hygiene, and batching are **MVP+** (add only after end-to-end is stable).

---

## Phase 0 — Dev environment (prep)

1) Pick a **network**: start with `testnet`.
2) Pick a **transport**: for MVP, the fastest path is to start with `soroban` CLI (subprocess) and later migrate to an SDK.
3) Put configuration behind env vars:

- `NC_SOROBAN_NETWORK` = `testnet`
- `NC_SOROBAN_RPC_URL` = testnet RPC
- `NC_SOROBAN_SECRET_KEY` or `NC_SOROBAN_KEYFILE` (never log)
- `NC_SOROBAN_ALLOWLIST` (contract IDs + functions)
- `NC_API_KEY` (server-only, optional)

**Definition of Done**
- You can run a CLI tool that prints “hello” and reads a `.nc` file.

---

## Phase 1 — Action schema + allowlist (core)

Create a clear action schema (enum/struct) in the shared core.

**Classic Stellar**
- `stellar.account.balance`
- `stellar.account.create`
- `stellar.account.fund_testnet`
- `stellar.change_trust`
- `stellar.payment`
- `stellar.tx.status`

**Soroban Smart**
- `soroban.contract.invoke`

Allowlist (MVP):
- Asset allowlist: only certain `asset_code + issuer`.
- Contract allowlist: only certain `contract_id + function`.

**Definition of Done**
- Action plans can be serialized (JSON) and validated (allowlist + required fields).

---

## Phase 2 — CLI skeleton: simulate → preview → confirm

Build a CLI binary (e.g. `neurochain-soroban`) that:

1) reads `.nc` / stdin
2) produces an action plan (initially “manual mode”: actions can be authored directly)
3) runs `simulate`
4) prints a preview (fee estimate + effects)
5) asks for confirm (Y/N)
6) submits or prints a tx hash

**Definition of Done**
- `BalanceQuery` or `TxStatus` works end-to-end.

---

## Phase 3 — Classic Stellar MVP (onboarding + payments)

Implement Classic operations one-by-one:

1) `FundTestnet` (Friendbot)
2) `BalanceQuery`
3) `TransferXLM`
4) `CreateAccount`
5) `ChangeTrust`
6) `TransferAsset`

Guardrails:
- Amount limits (max XLM / max asset)
- Trustline allowlist

**Definition of Done**
- A new account can be created/funded on testnet, a trustline can be added, and a payment can be executed.

---

## Phase 4 — Soroban MVP: ContractInvoke

Implement `ContractInvoke` like this:

1) allowlist contract + function
2) deterministic slot parsing for args
3) simulate → preview (fee, footprint, return/event size)
4) confirm → submit

**Definition of Done**
- One allowlisted contract can be invoked on testnet (e.g. a “hello/echo” style contract) reliably.

---

## Phase 5 — Axum server (same core)

Implement a server binary (e.g. `neurochain-soroban-server`) exposing:

- `POST /api/stellar/simulate`
- `POST /api/stellar/submit`
- `POST /api/soroban/simulate`
- `POST /api/soroban/submit`
- `GET /api/tx/status`

Security:
- `NC_API_KEY` (optional)
- concurrency limits
- request/output/log limits
- CORS (tighten for production)

**Definition of Done**
- A WebUI/HTTP client can run the same action plan as the CLI.

---

## Phase 6 — Intent model + slot parser (MVP automation)

Once manual actions are stable, add intent automation:

### Labels (MVP)

Classic Stellar:
- `BalanceQuery`
- `CreateAccount`
- `ChangeTrust`
- `TransferXLM`
- `TransferAsset`
- `FundTestnet`
- `TxStatus`

Soroban Smart:
- `ContractInvoke`

Label → action:
- `BalanceQuery` → `stellar.account.balance`
- `CreateAccount` → `stellar.account.create`
- `ChangeTrust` → `stellar.change_trust`
- `TransferXLM` → `stellar.payment` (asset=XLM)
- `TransferAsset` → `stellar.payment` (asset allowlist)
- `FundTestnet` → `stellar.account.fund_testnet`
- `TxStatus` → `stellar.tx.status`
- `ContractInvoke` → `soroban.contract.invoke`

Deterministic slot parsing:
- `amount`, `asset_code`, `asset_issuer`, `to`, `starting_balance`
- `account`, `contract_id`, `function`, `args`
- missing required slot → `Unknown` / no-op

**Definition of Done**
- `macro from AI: ...` produces the same action plan as the manual authoring path.

---

## Example scripts (.nc)

These examples show the target syntax and the desired action-plan shape.

### BalanceQuery (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Check XLM balance for G..."
# action plan (adapter):
# stellar.account.balance account="G..." asset="XLM"
```

### CreateAccount (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Create account G... with 2 XLM"
# action plan (adapter):
# stellar.account.create destination="G..." starting_balance="2"
```

### ChangeTrust (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Add trustline USDC from G... limit 1000"
# action plan (adapter):
# stellar.change_trust asset_code="USDC" asset_issuer="G..." limit="1000"
```

### TransferXLM (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Send 5 XLM to G..."
# action plan (adapter):
# stellar.payment to="G..." amount="5" asset="XLM"
```

### ContractInvoke (Soroban)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Invoke contract C... function transfer args: to=G..., amount=100"
# action plan (adapter):
# soroban.contract.invoke contract_id="C..." function="transfer" args="{to:G...,amount:100}"
```

### FundTestnet (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Fund testnet account G..."
# action plan (adapter):
# stellar.account.fund_testnet account="G..."
```

### TxStatus (Classic)
```nc
AI: "models/intent_stellar/model.onnx"
macro from AI: "Check tx status for hash ABC..."
# action plan (adapter):
# stellar.tx.status hash="ABC..."
```

---

## MVP+ (after the base path is stable)

Add only after the MVP is stable:

- Sponsorship / fee-bump (make `payer` explicit in preview)
- TTL/rent hygiene + fee component breakdown
- batching/atomicity (multi-op tx)
- deeper smart wallet/policy/require_auth paths
- passkey/WebAuthn flows (Launchtube/Mercury only if needed)
