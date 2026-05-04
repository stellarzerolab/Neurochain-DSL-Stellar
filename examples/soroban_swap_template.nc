# Soroban swap template showcase.
# Plan-only example: demonstrates ContractInvoke -> swap expansion via policy.
contract_policy: examples/soroban_swap_template_policy.json
AI: "models/intent_stellar/model.onnx"
intent_threshold: 0.00
set stellar intent from AI: "Invoke contract swap function swap amount 100 from USDC to XLM min_out 95 for wallet GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX"
