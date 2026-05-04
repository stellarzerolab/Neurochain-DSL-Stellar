# Soroban claim-rewards template showcase.
# Plan-only example: demonstrates ContractInvoke -> claim_rewards expansion via policy.
contract_policy: examples/soroban_claim_rewards_template_policy.json
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
set stellar intent from AI: "Invoke contract rewards function claim_rewards for wallet GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
