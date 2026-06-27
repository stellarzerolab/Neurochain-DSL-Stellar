# Guest boundary

The RISC Zero guest will:

1. read a typed ActionPlan, private policy and private audit nonce
2. validate canonical ordering and required typed fields
3. hash the ActionPlan and private policy preimages
4. evaluate allowlist, contract policy, intent safety and approval threshold
5. commit only the public journal

The guest must not sign, submit, broadcast, call x402 or contact a network.
Its image id is part of the public verification contract.
