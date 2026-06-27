use std::collections::HashSet;

use neurochain_zk_guardrail_contract::{
    DecisionStatus, Digest32, ExitCode, PrivatePolicy, ReasonCode, TypedActionPlan, TypedArg,
    TypedValue, CONTRACT_VERSION,
};
use neurochain_zk_guardrail_guest_adapter::{execute, GuestInput, GuestOutput, Sha256Provider};
use neurochain_zk_guardrail_host_adapter::{
    verify_attestation, HostVerificationError, ReceiptEnvelope, ReceiptVerifier, VerifiedNextStep,
};
use neurochain_zk_guardrail_soroban_boundary::{
    verify_and_consume, AttestationEnvelope, ContractNextStep, ContractRejection, NullifierStore,
    NullifierStoreError, ProofVerifier,
};

const IMAGE_ID: Digest32 = [0x11; 32];
const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const OTHER_CONTRACT: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const ALLOWED_CONTRACTS: &[&str] = &[CONTRACT];
const ALLOWED_CONTRACT_FUNCTIONS: &[&str] =
    &["CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ:purchase_credits"];
const ALLOWED_ASSETS: &[&str] = &["USDC"];
const ALLOWED_RECIPIENTS: &[&str] = &[RECIPIENT];
const FIXTURE_PROOF_DOMAIN: &[u8] = b"NC_ZK_FIXTURE_PROOF_V1\0";

struct FixtureOnlyDigest;

impl FixtureOnlyDigest {
    fn digest(input: &[u8]) -> Digest32 {
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

impl Sha256Provider for FixtureOnlyDigest {
    fn sha256(&self, input: &[u8]) -> Digest32 {
        Self::digest(input)
    }
}

struct FixtureOnlyProofVerifier;

impl FixtureOnlyProofVerifier {
    fn issue(image_id: &Digest32, journal_bytes: &[u8]) -> Vec<u8> {
        let mut preimage =
            Vec::with_capacity(FIXTURE_PROOF_DOMAIN.len() + image_id.len() + journal_bytes.len());
        preimage.extend_from_slice(FIXTURE_PROOF_DOMAIN);
        preimage.extend_from_slice(image_id);
        preimage.extend_from_slice(journal_bytes);
        FixtureOnlyDigest::digest(&preimage).to_vec()
    }

    fn verify_fixture(image_id: &Digest32, journal_bytes: &[u8], proof: &[u8]) -> Result<(), ()> {
        if proof == Self::issue(image_id, journal_bytes) {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl ReceiptVerifier for FixtureOnlyProofVerifier {
    type Error = ();

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        receipt_seal: &[u8],
    ) -> Result<(), Self::Error> {
        Self::verify_fixture(expected_image_id, journal_bytes, receipt_seal)
    }
}

impl ProofVerifier for FixtureOnlyProofVerifier {
    type Error = ();

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        proof: &[u8],
    ) -> Result<(), Self::Error> {
        Self::verify_fixture(expected_image_id, journal_bytes, proof)
    }
}

#[derive(Default)]
struct FixtureNullifierStore {
    consumed: HashSet<Digest32>,
    attempts: usize,
}

impl NullifierStore for FixtureNullifierStore {
    fn consume_if_unused(&mut self, audit_nullifier: Digest32) -> Result<(), NullifierStoreError> {
        self.attempts += 1;
        if !self.consumed.insert(audit_nullifier) {
            return Err(NullifierStoreError::AlreadyConsumed);
        }
        Ok(())
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

fn guest_output<'a>(
    action_plan: &'a TypedActionPlan<'a>,
    private_policy: &'a PrivatePolicy<'a>,
    audit_nonce: Digest32,
) -> GuestOutput {
    execute(
        GuestInput {
            evaluator_image_id: IMAGE_ID,
            action_plan,
            private_policy,
            audit_nonce,
        },
        &FixtureOnlyDigest,
    )
    .expect("canonical fixture input should execute")
}

fn verify_host(output: &GuestOutput, proof: &[u8]) -> VerifiedNextStep {
    verify_attestation(
        IMAGE_ID,
        ReceiptEnvelope {
            journal_bytes: &output.journal_bytes,
            receipt_seal: proof,
        },
        &FixtureOnlyProofVerifier,
    )
    .expect("fixture proof should pass host boundary")
    .next_step()
}

fn verify_contract(
    output: &GuestOutput,
    proof: &[u8],
    store: &mut FixtureNullifierStore,
) -> ContractNextStep {
    verify_and_consume(
        IMAGE_ID,
        AttestationEnvelope {
            journal_bytes: &output.journal_bytes,
            proof,
        },
        &FixtureOnlyProofVerifier,
        store,
    )
    .expect("fixture proof should pass contract boundary")
    .next_step()
}

fn assert_pipeline(
    output: &GuestOutput,
    expected_status: DecisionStatus,
    expected_exit: ExitCode,
    expected_reason: ReasonCode,
    expected_host: VerifiedNextStep,
    expected_contract: ContractNextStep,
) {
    assert_eq!(output.journal.decision_status, expected_status);
    assert_eq!(output.journal.exit_code, expected_exit);
    assert_eq!(output.journal.reason_code, expected_reason);

    let proof = FixtureOnlyProofVerifier::issue(&IMAGE_ID, &output.journal_bytes);
    assert_eq!(verify_host(output, &proof), expected_host);

    let mut store = FixtureNullifierStore::default();
    assert_eq!(
        verify_contract(output, &proof, &mut store),
        expected_contract
    );
    assert_eq!(store.attempts, 1);
}

#[test]
fn approved_pipeline_binds_commitments_without_revealing_policy_values() {
    let args = args(500_000_000);
    let action = plan(&args, CONTRACT, 9_800);
    let private_policy = policy(1_000_000_000, 600_000_000);
    let output = guest_output(&action, &private_policy, [0x21; 32]);

    assert_pipeline(
        &output,
        DecisionStatus::Approved,
        ExitCode::Passed,
        ReasonCode::Passed,
        VerifiedNextStep::EligibleForSeparateApprovalFlow,
        ContractNextStep::EligibleForSeparateApprovalFlow,
    );
    assert!(!output
        .journal_bytes
        .windows(CONTRACT.len())
        .any(|window| window == CONTRACT.as_bytes()));
    assert!(!output
        .journal_bytes
        .windows(RECIPIENT.len())
        .any(|window| window == RECIPIENT.as_bytes()));
    assert!(!output
        .journal_bytes
        .windows(b"USDC".len())
        .any(|window| window == b"USDC"));
}

#[test]
fn requires_approval_and_exit_3_4_5_remain_non_submit_end_to_end() {
    let standard_args = args(500_000_000);
    let standard = plan(&standard_args, CONTRACT, 9_800);
    let requires_approval =
        guest_output(&standard, &policy(1_000_000_000, 400_000_000), [0x31; 32]);
    assert_pipeline(
        &requires_approval,
        DecisionStatus::RequiresApproval,
        ExitCode::Passed,
        ReasonCode::ApprovalThreshold,
        VerifiedNextStep::RequiresApproval,
        ContractNextStep::RequiresApproval,
    );

    let disallowed = plan(&standard_args, OTHER_CONTRACT, 9_800);
    let exit_3 = guest_output(&disallowed, &policy(1_000_000_000, 600_000_000), [0x32; 32]);
    assert_pipeline(
        &exit_3,
        DecisionStatus::Blocked,
        ExitCode::Allowlist,
        ReasonCode::Allowlist,
        VerifiedNextStep::Blocked,
        ContractNextStep::Blocked,
    );

    let exit_4 = guest_output(&standard, &policy(250_000_000, 200_000_000), [0x33; 32]);
    assert_pipeline(
        &exit_4,
        DecisionStatus::Blocked,
        ExitCode::ContractPolicy,
        ReasonCode::ContractPolicy,
        VerifiedNextStep::Blocked,
        ContractNextStep::Blocked,
    );

    let missing_recipient_args = &standard_args[..2];
    let missing_recipient = plan(missing_recipient_args, CONTRACT, 9_800);
    let exit_5 = guest_output(
        &missing_recipient,
        &policy(1_000_000_000, 600_000_000),
        [0x34; 32],
    );
    assert_pipeline(
        &exit_5,
        DecisionStatus::Blocked,
        ExitCode::IntentSafety,
        ReasonCode::IntentSafety,
        VerifiedNextStep::Blocked,
        ContractNextStep::Blocked,
    );
}

#[test]
fn tampering_after_fixture_proof_is_rejected_before_replay_state() {
    let args = args(500_000_000);
    let action = plan(&args, CONTRACT, 9_800);
    let mut output = guest_output(&action, &policy(1_000_000_000, 600_000_000), [0x41; 32]);
    let proof = FixtureOnlyProofVerifier::issue(&IMAGE_ID, &output.journal_bytes);
    output.journal_bytes[0] ^= 1;

    assert_eq!(
        verify_attestation(
            IMAGE_ID,
            ReceiptEnvelope {
                journal_bytes: &output.journal_bytes,
                receipt_seal: &proof,
            },
            &FixtureOnlyProofVerifier,
        ),
        Err(HostVerificationError::ReceiptRejected)
    );

    let mut store = FixtureNullifierStore::default();
    assert_eq!(
        verify_and_consume(
            IMAGE_ID,
            AttestationEnvelope {
                journal_bytes: &output.journal_bytes,
                proof: &proof,
            },
            &FixtureOnlyProofVerifier,
            &mut store,
        ),
        Err(ContractRejection {
            exit_code: ExitCode::ContractPolicy,
            reason_code: ReasonCode::InvalidAttestation,
        })
    );
    assert_eq!(store.attempts, 0);
    assert!(store.consumed.is_empty());
}

#[test]
fn valid_attestation_is_consumed_once_then_rejected_as_replay() {
    let args = args(500_000_000);
    let action = plan(&args, CONTRACT, 9_800);
    let output = guest_output(&action, &policy(1_000_000_000, 600_000_000), [0x51; 32]);
    let proof = FixtureOnlyProofVerifier::issue(&IMAGE_ID, &output.journal_bytes);
    let mut store = FixtureNullifierStore::default();

    assert_eq!(
        verify_contract(&output, &proof, &mut store),
        ContractNextStep::EligibleForSeparateApprovalFlow
    );
    assert_eq!(
        verify_and_consume(
            IMAGE_ID,
            AttestationEnvelope {
                journal_bytes: &output.journal_bytes,
                proof: &proof,
            },
            &FixtureOnlyProofVerifier,
            &mut store,
        ),
        Err(ContractRejection {
            exit_code: ExitCode::ContractPolicy,
            reason_code: ReasonCode::Replay,
        })
    );
    assert_eq!(store.attempts, 2);
}
