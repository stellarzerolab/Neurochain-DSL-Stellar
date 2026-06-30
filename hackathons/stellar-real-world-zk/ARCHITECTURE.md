# Architecture

## Proof and verification flow

```mermaid
flowchart LR
    A["Agent intent"] --> B["NeuroChain typed ActionPlan"]
    B --> C["RISC Zero guest"]
    P["Private owner policy"] --> C
    N["Private audit nonce"] --> C
    C --> J["Canonical public journal"]
    C --> R["Groth16 receipt"]
    J --> S["Soroban guardrail application"]
    R --> V["Verifier router"]
    V --> G["Groth16 verifier"]
    G --> S
    S --> D["approved | requires_approval | blocked"]
    S --> Q["Persistent consumed nullifier"]
```

## Trust boundaries

### Private witness

The owner policy, commitment salt and audit nonce enter only the RISC Zero
guest. They are used to compute the policy commitment, decision and audit
nullifier but are not written to the public artifact.

### Committed evaluator

The RISC Zero image ID identifies the exact guest program. Soroban is
configured with that expected image ID and rejects a journal bound to another
evaluator.

### Public journal

The journal contains only commitments and the result. Canonical encoding and a
strict shared decoder prevent JSON formatting, key-order or parser ambiguity.

### Soroban verification

The application hashes the received journal, verifies the Groth16 seal through
the pinned router, checks the image binding, decodes the decision and consumes
the nullifier. Proof failure occurs before replay state is read or written.

## Decision boundary

```mermaid
flowchart TD
    V["Valid proof and unused nullifier"] --> X{"Attested decision"}
    X -->|"approved / exit 0"| A["Eligible for a separate approval flow"]
    X -->|"requires_approval / exit 0"| R["Human or owner approval required"]
    X -->|"blocked / exit 3, 4 or 5"| B["Blocked"]
    I["Invalid proof or replay"] --> E["Contract rejection"]
    A --> Z["No automatic submit"]
    R --> Z
    B --> Z
```

Payment and proof verification are authorization inputs, not transaction
submission. Signing and broadcasting remain outside this hackathon package.
