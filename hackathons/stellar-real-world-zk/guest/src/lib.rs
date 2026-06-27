use neurochain_zk_guardrail_contract::{
    audit_nullifier_preimage, evaluate, ContractError, Digest32, PrivatePolicy, PublicJournal,
    TypedActionPlan,
};

pub trait Sha256Provider {
    fn sha256(&self, input: &[u8]) -> Digest32;
}

#[derive(Debug, Clone, Copy)]
pub struct GuestInput<'a> {
    pub evaluator_image_id: Digest32,
    pub action_plan: &'a TypedActionPlan<'a>,
    pub private_policy: &'a PrivatePolicy<'a>,
    pub audit_nonce: Digest32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestOutput {
    pub journal: PublicJournal,
    pub journal_bytes: Vec<u8>,
}

pub fn execute(
    input: GuestInput<'_>,
    sha256: &impl Sha256Provider,
) -> Result<GuestOutput, ContractError> {
    let action_plan_preimage = input.action_plan.canonical_preimage()?;
    let policy_preimage = input.private_policy.canonical_preimage()?;

    let action_plan_hash = sha256.sha256(&action_plan_preimage);
    let policy_commitment = sha256.sha256(&policy_preimage);
    let nullifier_preimage = audit_nullifier_preimage(
        &input.evaluator_image_id,
        &action_plan_hash,
        &policy_commitment,
        &input.audit_nonce,
    );
    let audit_nullifier = sha256.sha256(&nullifier_preimage);

    let decision = evaluate(input.action_plan, input.private_policy);
    let journal = decision.into_journal(
        input.evaluator_image_id,
        action_plan_hash,
        policy_commitment,
        input.private_policy.policy_version,
        audit_nullifier,
    );
    let journal_bytes = journal.encode()?;

    Ok(GuestOutput {
        journal,
        journal_bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use neurochain_zk_guardrail_contract::{
        DecisionStatus, ExitCode, ReasonCode, TypedArg, TypedValue, CONTRACT_VERSION,
    };

    use super::*;

    const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    const OTHER_CONTRACT: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    const ALLOWED_CONTRACTS: &[&str] = &[CONTRACT];
    const ALLOWED_CONTRACT_FUNCTIONS: &[&str] =
        &["CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ:purchase_credits"];
    const ALLOWED_ASSETS: &[&str] = &["USDC"];
    const ALLOWED_RECIPIENTS: &[&str] = &[RECIPIENT];

    #[derive(Default)]
    struct RecordingTestDigest {
        inputs: RefCell<Vec<Vec<u8>>>,
    }

    impl RecordingTestDigest {
        fn digest_bytes(input: &[u8]) -> Digest32 {
            let mut output = [0u8; 32];
            for (index, byte) in input.iter().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_mul(31)
                    .wrapping_add(*byte)
                    .wrapping_add(index as u8);
            }
            output
        }
    }

    impl Sha256Provider for RecordingTestDigest {
        fn sha256(&self, input: &[u8]) -> Digest32 {
            self.inputs.borrow_mut().push(input.to_vec());
            Self::digest_bytes(input)
        }
    }

    fn args(amount: u64) -> [TypedArg<'static>; 3] {
        [
            TypedArg {
                name: "amount",
                value: TypedValue::U64(amount),
            },
            TypedArg {
                name: "asset",
                value: TypedValue::Symbol("USDC"),
            },
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
        ]
    }

    fn plan<'a>(
        args: &'a [TypedArg<'a>],
        contract_id: &'a str,
        confidence_bps: u16,
    ) -> TypedActionPlan<'a> {
        TypedActionPlan {
            schema_version: CONTRACT_VERSION,
            intent_label: "ContractInvoke",
            action_kind: "soroban_contract_invoke",
            contract_id,
            function: "purchase_credits",
            args,
            intent_confidence_bps: confidence_bps,
        }
    }

    fn policy(max_amount_minor: u64, approval_threshold_minor: u64) -> PrivatePolicy<'static> {
        PrivatePolicy {
            schema_version: CONTRACT_VERSION,
            policy_version: 7,
            commitment_salt: [0x55; 32],
            allowed_contracts: ALLOWED_CONTRACTS,
            allowed_contract_functions: ALLOWED_CONTRACT_FUNCTIONS,
            allowed_assets: ALLOWED_ASSETS,
            allowed_recipients: ALLOWED_RECIPIENTS,
            max_amount_minor,
            approval_threshold_minor,
            min_intent_confidence_bps: 9_000,
        }
    }

    fn run<'a>(
        action_plan: &'a TypedActionPlan<'a>,
        private_policy: &'a PrivatePolicy<'a>,
        digest: &impl Sha256Provider,
    ) -> GuestOutput {
        execute(
            GuestInput {
                evaluator_image_id: [0x11; 32],
                action_plan,
                private_policy,
                audit_nonce: [0x22; 32],
            },
            digest,
        )
        .unwrap()
    }

    #[test]
    fn guest_adapter_binds_plan_policy_nullifier_and_journal() {
        let args = args(500_000_000);
        let action = plan(&args, CONTRACT, 9_800);
        let private_policy = policy(1_000_000_000, 600_000_000);
        let digest = RecordingTestDigest::default();
        let output = run(&action, &private_policy, &digest);

        let recorded = digest.inputs.borrow();
        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[0], action.canonical_preimage().unwrap());
        assert_eq!(recorded[1], private_policy.canonical_preimage().unwrap());
        assert_eq!(
            output.journal.action_plan_hash,
            RecordingTestDigest::digest_bytes(&recorded[0])
        );
        assert_eq!(
            output.journal.policy_commitment,
            RecordingTestDigest::digest_bytes(&recorded[1])
        );
        assert_eq!(
            output.journal.audit_nullifier,
            RecordingTestDigest::digest_bytes(&recorded[2])
        );
        assert_eq!(output.journal.policy_version, 7);
        assert_eq!(output.journal.decision_status, DecisionStatus::Approved);
        assert_eq!(output.journal.exit_code, ExitCode::Passed);
        assert_eq!(output.journal.reason_code, ReasonCode::Passed);
        assert_eq!(output.journal_bytes, output.journal.encode().unwrap());
    }

    #[test]
    fn guest_adapter_preserves_decision_matrix() {
        let args = args(500_000_000);
        let approved_plan = plan(&args, CONTRACT, 9_800);

        let approved = run(
            &approved_plan,
            &policy(1_000_000_000, 600_000_000),
            &RecordingTestDigest::default(),
        );
        assert_eq!(approved.journal.decision_status, DecisionStatus::Approved);

        let requires_approval = run(
            &approved_plan,
            &policy(1_000_000_000, 400_000_000),
            &RecordingTestDigest::default(),
        );
        assert_eq!(
            requires_approval.journal.decision_status,
            DecisionStatus::RequiresApproval
        );
        assert!(requires_approval.journal.requires_approval);

        let exit_3_plan = plan(&args, OTHER_CONTRACT, 9_800);
        let exit_3 = run(
            &exit_3_plan,
            &policy(1_000_000_000, 600_000_000),
            &RecordingTestDigest::default(),
        );
        assert_eq!(exit_3.journal.exit_code, ExitCode::Allowlist);

        let exit_4 = run(
            &approved_plan,
            &policy(250_000_000, 200_000_000),
            &RecordingTestDigest::default(),
        );
        assert_eq!(exit_4.journal.exit_code, ExitCode::ContractPolicy);

        let missing_recipient_args = &args[..2];
        let exit_5_plan = plan(missing_recipient_args, CONTRACT, 9_800);
        let exit_5 = run(
            &exit_5_plan,
            &policy(1_000_000_000, 600_000_000),
            &RecordingTestDigest::default(),
        );
        assert_eq!(exit_5.journal.exit_code, ExitCode::IntentSafety);
    }

    #[test]
    fn changing_action_plan_changes_hash_and_nullifier() {
        let first_args = args(500_000_000);
        let second_args = args(500_000_001);
        let first_plan = plan(&first_args, CONTRACT, 9_800);
        let second_plan = plan(&second_args, CONTRACT, 9_800);
        let private_policy = policy(1_000_000_000, 600_000_000);

        let first = run(
            &first_plan,
            &private_policy,
            &RecordingTestDigest::default(),
        );
        let second = run(
            &second_plan,
            &private_policy,
            &RecordingTestDigest::default(),
        );

        assert_ne!(
            first.journal.action_plan_hash,
            second.journal.action_plan_hash
        );
        assert_eq!(
            first.journal.policy_commitment,
            second.journal.policy_commitment
        );
        assert_ne!(
            first.journal.audit_nullifier,
            second.journal.audit_nullifier
        );
    }

    #[test]
    fn noncanonical_input_fails_before_any_digest() {
        let args = [
            TypedArg {
                name: "recipient",
                value: TypedValue::Address(RECIPIENT),
            },
            TypedArg {
                name: "amount",
                value: TypedValue::U64(500_000_000),
            },
        ];
        let action = plan(&args, CONTRACT, 9_800);
        let private_policy = policy(1_000_000_000, 600_000_000);
        let digest = RecordingTestDigest::default();

        let result = execute(
            GuestInput {
                evaluator_image_id: [0x11; 32],
                action_plan: &action,
                private_policy: &private_policy,
                audit_nonce: [0x22; 32],
            },
            &digest,
        );

        assert_eq!(result, Err(ContractError::NonCanonicalOrder));
        assert!(digest.inputs.borrow().is_empty());
    }
}
