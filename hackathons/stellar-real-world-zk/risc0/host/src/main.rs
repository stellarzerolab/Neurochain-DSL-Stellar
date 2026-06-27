use std::collections::HashSet;

use neurochain_zk_guardrail_contract::{
    DecisionStatus, Digest32, ExitCode, ReasonCode, CONTRACT_VERSION,
};
use neurochain_zk_guardrail_host_adapter::{
    verify_attestation, ReceiptEnvelope, ReceiptVerifier, VerifiedNextStep,
};
use neurochain_zk_guardrail_soroban_boundary::{
    verify_and_consume, AttestationEnvelope, ContractNextStep, NullifierStore, NullifierStoreError,
    ProofVerifier,
};
use neurochain_zk_risc0_methods::{NEUROCHAIN_ZK_RISC0_GUEST_ELF, NEUROCHAIN_ZK_RISC0_GUEST_ID};
use neurochain_zk_risc0_types::{
    OwnedActionPlan, OwnedPrivatePolicy, OwnedTypedArg, OwnedTypedValue, ProofInput,
};
use risc0_zkvm::{default_prover, sha::Digest, ExecutorEnv, Receipt};

const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";

struct RealReceiptVerifier {
    method_id: Digest,
    image_id_bytes: Digest32,
}

impl RealReceiptVerifier {
    fn verify_receipt(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        proof: &[u8],
    ) -> Result<(), String> {
        if proof.is_empty() || expected_image_id != &self.image_id_bytes {
            return Err("receipt envelope does not match the proven receipt".to_owned());
        }
        let receipt: Receipt = bincode::deserialize(proof).map_err(|error| error.to_string())?;
        if journal_bytes != receipt.journal.bytes.as_slice() {
            return Err("journal does not match the serialized receipt".to_owned());
        }
        receipt
            .verify(self.method_id)
            .map_err(|error| error.to_string())
    }
}

impl ReceiptVerifier for RealReceiptVerifier {
    type Error = String;

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        receipt_seal: &[u8],
    ) -> Result<(), Self::Error> {
        self.verify_receipt(expected_image_id, journal_bytes, receipt_seal)
    }
}

impl ProofVerifier for RealReceiptVerifier {
    type Error = String;

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        proof: &[u8],
    ) -> Result<(), Self::Error> {
        self.verify_receipt(expected_image_id, journal_bytes, proof)
    }
}

#[derive(Default)]
struct InMemoryNullifiers {
    consumed: HashSet<Digest32>,
}

impl NullifierStore for InMemoryNullifiers {
    fn consume_if_unused(&mut self, audit_nullifier: Digest32) -> Result<(), NullifierStoreError> {
        if !self.consumed.insert(audit_nullifier) {
            return Err(NullifierStoreError::AlreadyConsumed);
        }
        Ok(())
    }
}

fn digest_bytes(digest: Digest) -> Digest32 {
    let mut output = [0u8; 32];
    output.copy_from_slice(digest.as_bytes());
    output
}

fn approved_input(evaluator_image_id: Digest32) -> ProofInput {
    ProofInput {
        evaluator_image_id,
        action_plan: OwnedActionPlan {
            schema_version: CONTRACT_VERSION,
            intent_label: "ContractInvoke".to_owned(),
            action_kind: "soroban_contract_invoke".to_owned(),
            contract_id: CONTRACT.to_owned(),
            function: "purchase_credits".to_owned(),
            args: vec![
                OwnedTypedArg {
                    name: "amount".to_owned(),
                    value: OwnedTypedValue::U64(500_000_000),
                },
                OwnedTypedArg {
                    name: "asset".to_owned(),
                    value: OwnedTypedValue::Symbol("USDC".to_owned()),
                },
                OwnedTypedArg {
                    name: "recipient".to_owned(),
                    value: OwnedTypedValue::Address(RECIPIENT.to_owned()),
                },
            ],
            intent_confidence_bps: 9_800,
        },
        private_policy: OwnedPrivatePolicy {
            schema_version: CONTRACT_VERSION,
            policy_version: 7,
            commitment_salt: [0x55; 32],
            allowed_contracts: vec![CONTRACT.to_owned()],
            allowed_contract_functions: vec![format!("{CONTRACT}:purchase_credits")],
            allowed_assets: vec!["USDC".to_owned()],
            allowed_recipients: vec![RECIPIENT.to_owned()],
            max_amount_minor: 1_000_000_000,
            approval_threshold_minor: 600_000_000,
            min_intent_confidence_bps: 9_000,
        },
        audit_nonce: [0x22; 32],
    }
}

fn main() {
    let method_id = Digest::from(NEUROCHAIN_ZK_RISC0_GUEST_ID);
    let evaluator_image_id = digest_bytes(method_id);
    let input = approved_input(evaluator_image_id);
    let env = ExecutorEnv::builder()
        .write(&input)
        .expect("proof input serialization must succeed")
        .build()
        .expect("executor environment must build");

    let receipt = default_prover()
        .prove(env, NEUROCHAIN_ZK_RISC0_GUEST_ELF)
        .expect("RISC Zero proving must succeed")
        .receipt;
    receipt
        .verify(method_id)
        .expect("generated receipt must verify against the guest image id");
    let receipt_bytes =
        bincode::serialize(&receipt).expect("genuine receipt serialization must succeed");

    let verifier = RealReceiptVerifier {
        method_id,
        image_id_bytes: evaluator_image_id,
    };
    let journal_bytes = receipt.journal.bytes.as_slice();
    let host_verified = verify_attestation(
        evaluator_image_id,
        ReceiptEnvelope {
            journal_bytes,
            receipt_seal: &receipt_bytes,
        },
        &verifier,
    )
    .expect("host receipt boundary must accept the genuine receipt");
    assert_eq!(
        host_verified.journal.decision_status,
        DecisionStatus::Approved
    );
    assert_eq!(host_verified.journal.exit_code, ExitCode::Passed);
    assert_eq!(host_verified.journal.reason_code, ReasonCode::Passed);
    assert_eq!(
        host_verified.next_step(),
        VerifiedNextStep::EligibleForSeparateApprovalFlow
    );

    let mut nullifiers = InMemoryNullifiers::default();
    let contract_verified = verify_and_consume(
        evaluator_image_id,
        AttestationEnvelope {
            journal_bytes,
            proof: &receipt_bytes,
        },
        &verifier,
        &mut nullifiers,
    )
    .expect("contract boundary must accept the genuine receipt once");
    assert_eq!(
        contract_verified.next_step(),
        ContractNextStep::EligibleForSeparateApprovalFlow
    );

    let replay = verify_and_consume(
        evaluator_image_id,
        AttestationEnvelope {
            journal_bytes,
            proof: &receipt_bytes,
        },
        &verifier,
        &mut nullifiers,
    )
    .expect_err("the same audit nullifier must be rejected on replay");
    assert_eq!(replay.exit_code, ExitCode::ContractPolicy);
    assert_eq!(replay.reason_code, ReasonCode::Replay);

    println!("receipt_verified=true");
    println!("decision=approved");
    println!("exit_code=0");
    println!("next_step=eligible_for_separate_approval_flow");
    println!("replay=blocked_exit_4");
}
