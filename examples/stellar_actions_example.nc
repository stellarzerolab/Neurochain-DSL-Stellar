# Manual action-plan example for the Soroban adapter
# These lines are parsed by parse_action_plan_from_nc() and printed as JSON by neurochain-soroban.

stellar.account.balance account="G..." asset="XLM"

stellar.account.create destination="G..." starting_balance="2"
stellar.account.fund_testnet account="G..."

stellar.change_trust asset_code="USDC" asset_issuer="G..." limit="1000"

stellar.payment to="G..." amount="5" asset_code="XLM"
stellar.payment to="G..." amount="12.5" asset_code="USDC" asset_issuer="G..."

stellar.tx.status hash="ABC123"

soroban.contract.invoke contract_id="C..." function="transfer" args={"to":"G...","amount":100}
