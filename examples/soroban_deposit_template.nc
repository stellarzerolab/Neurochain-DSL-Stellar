# Soroban deposit template showcase.
# Plan-only example: demonstrates ContractInvoke -> deposit expansion via policy.
contract_policy: examples/soroban_deposit_template_policy.json
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
set stellar intent from AI: "Invoke contract deposit function deposit 100 for wallet GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
