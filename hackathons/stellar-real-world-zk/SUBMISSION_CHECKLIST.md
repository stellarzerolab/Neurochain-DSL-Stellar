# Submission Checklist

## Repository package

- [x] Open-source implementation is in the public Stellar repository.
- [x] Judge-facing problem, ZK value and Stellar integration are documented.
- [x] Architecture and trust boundaries are diagrammed.
- [x] Reproduction commands use real RISC Zero and Soroban verification.
- [x] Public proof fixtures cover approved, requires approval and exit `3`.
- [x] Replay and invalid-proof rejection are demonstrated in Protocol 26
  localnet.
- [x] Security limitations and unaudited verifier status are disclosed.
- [x] No private policy values, wallet secrets or local-only files are included.
- [x] One-command proof-only video rehearsal is available without Stellar
  network or wallet activity.

Run the package gate:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/check_submission_package.ps1 -RunTests
```

Machine-readable result:

```powershell
powershell -ExecutionPolicy Bypass -File hackathons/stellar-real-world-zk/scripts/check_submission_package.ps1 -Format Json
```

## Manual submission items

- [ ] Record the 2-3 minute demo using `DEMO_SCRIPT.md`.
- [ ] Upload the video and add its final public URL to the DoraHacks entry.
- [ ] Confirm the repository URL and submission description in DoraHacks.
- [ ] Confirm the final deadline shown by the DoraHacks submission UI.
- [ ] Submit the BUIDL and read back the published entry.

## Optional testnet evidence

- [ ] Decide whether testnet evidence adds enough value beyond the complete
  Protocol 26 localnet proof.
- [ ] Obtain explicit approval before any testnet deploy or submit.
- [ ] If approved, use a dedicated test identity and record contract IDs and
  transaction links without storing secret material.

The package gate validates repository evidence only. It does not claim that
the video, DoraHacks form or optional testnet step has been completed.
