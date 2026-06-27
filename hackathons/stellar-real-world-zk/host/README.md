# Host boundary

The host will adapt the existing NeuroChain `ContractInvoke` ActionPlan into
the canonical shared representation, provide private guest inputs, generate a
receipt and decode the public journal.

The host must verify the receipt locally before forwarding it. It must never
interpret `approved` or a valid receipt as automatic submit permission.
