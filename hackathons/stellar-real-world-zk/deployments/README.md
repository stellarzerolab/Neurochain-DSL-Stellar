# Deployment manifests

This directory holds public, secret-free deployment evidence for the ZK
Guardrail demo.

`testnet.json` is created only by an explicitly authorized testnet deployment:

```powershell
powershell -ExecutionPolicy Bypass -File `
  hackathons/stellar-real-world-zk/scripts/deploy_testnet.ps1 `
  -Source nc-zk-testnet `
  -VerifierWasm path/to/groth16_verifier.wasm `
  -RouterWasm path/to/risc0_router.wasm `
  -Execute
```

The script is testnet-only, requires `-Execute`, verifies pinned WASM hashes,
authorizes the three bundled policy commitments, performs read-only Soroban
verification for every demo proof, and writes contract IDs plus reproducibility
metadata. It never writes a secret key, seed phrase, or key material.
