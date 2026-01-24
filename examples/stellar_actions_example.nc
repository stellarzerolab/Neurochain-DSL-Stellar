# Manual action-plan example for the Soroban adapter
# These lines are parsed by parse_action_plan_from_nc() and printed as JSON by neurochain-soroban.

stellar.account.balance account="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" asset="XLM"

stellar.account.create destination="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" starting_balance="2"
stellar.account.fund_testnet account="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P"

stellar.change_trust asset_code="USDC" asset_issuer="G..." limit="1000"

stellar.payment to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="5" asset_code="XLM"
stellar.payment to="GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P" amount="12.5" asset_code="USDC" asset_issuer="G..."

stellar.tx.status hash="ABC123"

# soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"GBSBBQGSMZEZJLPCQZFIDSEUSUEZVKP3KHS3JKV27BSWWTUL35VEL72P","amount":100}
