# Host boundary

The host will adapt the existing NeuroChain `ContractInvoke` ActionPlan into
the canonical shared representation, provide private guest inputs and generate
a receipt.

The dependency-free adapter in `src/lib.rs` requires a `ReceiptVerifier`
implementation, strictly decodes the canonical public journal and checks that
the journal image id equals the verifier-approved expected image id. Empty
seals, verifier errors, malformed journals and image-id mismatches fail closed.

The host must verify the receipt locally before forwarding it. It must never
interpret `approved` or a valid receipt as automatic submit permission.
