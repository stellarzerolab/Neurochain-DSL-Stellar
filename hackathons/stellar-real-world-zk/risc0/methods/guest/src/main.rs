use neurochain_zk_guardrail_contract::{
    Digest32, PrivatePolicy, TypedActionPlan, TypedArg, TypedValue,
};
use neurochain_zk_guardrail_guest_adapter::{execute, GuestInput, Sha256Provider};
use neurochain_zk_risc0_types::{OwnedTypedValue, ProofInput};
use risc0_zkvm::{
    guest::env,
    sha::{Impl, Sha256},
};

struct RiscZeroSha256;

impl Sha256Provider for RiscZeroSha256 {
    fn sha256(&self, input: &[u8]) -> Digest32 {
        let digest = Impl::hash_bytes(input);
        let mut output = [0u8; 32];
        output.copy_from_slice(digest.as_bytes());
        output
    }
}

fn main() {
    let input: ProofInput = env::read();

    let args: Vec<TypedArg<'_>> = input
        .action_plan
        .args
        .iter()
        .map(|arg| TypedArg {
            name: arg.name.as_str(),
            value: match &arg.value {
                OwnedTypedValue::Address(value) => TypedValue::Address(value.as_str()),
                OwnedTypedValue::Bytes(value) => TypedValue::Bytes(value.as_slice()),
                OwnedTypedValue::Symbol(value) => TypedValue::Symbol(value.as_str()),
                OwnedTypedValue::U64(value) => TypedValue::U64(*value),
            },
        })
        .collect();
    let allowed_contracts: Vec<&str> = input
        .private_policy
        .allowed_contracts
        .iter()
        .map(String::as_str)
        .collect();
    let allowed_contract_functions: Vec<&str> = input
        .private_policy
        .allowed_contract_functions
        .iter()
        .map(String::as_str)
        .collect();
    let allowed_assets: Vec<&str> = input
        .private_policy
        .allowed_assets
        .iter()
        .map(String::as_str)
        .collect();
    let allowed_recipients: Vec<&str> = input
        .private_policy
        .allowed_recipients
        .iter()
        .map(String::as_str)
        .collect();

    let action_plan = TypedActionPlan {
        schema_version: input.action_plan.schema_version,
        intent_label: input.action_plan.intent_label.as_str(),
        action_kind: input.action_plan.action_kind.as_str(),
        contract_id: input.action_plan.contract_id.as_str(),
        function: input.action_plan.function.as_str(),
        args: &args,
        intent_confidence_bps: input.action_plan.intent_confidence_bps,
    };
    let private_policy = PrivatePolicy {
        schema_version: input.private_policy.schema_version,
        policy_version: input.private_policy.policy_version,
        commitment_salt: input.private_policy.commitment_salt,
        allowed_contracts: &allowed_contracts,
        allowed_contract_functions: &allowed_contract_functions,
        allowed_assets: &allowed_assets,
        allowed_recipients: &allowed_recipients,
        max_amount_minor: input.private_policy.max_amount_minor,
        approval_threshold_minor: input.private_policy.approval_threshold_minor,
        min_intent_confidence_bps: input.private_policy.min_intent_confidence_bps,
    };

    let output = execute(
        GuestInput {
            evaluator_image_id: input.evaluator_image_id,
            action_plan: &action_plan,
            private_policy: &private_policy,
            audit_nonce: input.audit_nonce,
        },
        &RiscZeroSha256,
    )
    .expect("canonical NeuroChain guardrail evaluation must succeed");

    env::commit_slice(&output.journal_bytes);
}
