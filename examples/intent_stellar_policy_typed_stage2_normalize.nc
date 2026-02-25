# IntentStellar policy-backed typed v2 stage2 normalization demo (pass)
#
# Goal:
# - Contract policy requires `hello.to` to be a `symbol`
# - Prompt gives `args={"to":" World "}` (extra whitespace)
# - Stage2 typed normalization trims the value -> `"World"`
# - Result: action remains `soroban_contract_invoke` (no `slot_type_error`)
#
# Run (plan-only, default file mode without --flow):
#   cargo run --release --bin neurochain-stellar -- examples\\intent_stellar_policy_typed_stage2_normalize.nc

AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
contract_policy: contracts/CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ/policy.json

set stellar intent from AI: "Invoke contract CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ function hello args={"to":" World "}"
