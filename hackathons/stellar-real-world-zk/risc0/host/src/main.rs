use std::{collections::HashSet, env, fs, path::Path};

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
use risc0_zkvm::{default_prover, sha::Digest, ExecutorEnv, InnerReceipt, ProverOpts, Receipt};
use serde::Serialize;
use sha2::{Digest as Sha2Digest, Sha256};

const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const BLOCKED_CONTRACT: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const APPROVED_PROOF_ARTIFACT: &str = "target/neurochain-zk-stellar-proof.json";
const REQUIRES_APPROVAL_PROOF_ARTIFACT: &str =
    "target/neurochain-zk-stellar-proof-requires-approval.json";
const BLOCKED_ALLOWLIST_PROOF_ARTIFACT: &str =
    "target/neurochain-zk-stellar-proof-blocked-allowlist.json";

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Approved,
    RequiresApproval,
    BlockedAllowlist,
}

impl Scenario {
    fn from_args() -> Result<Self, String> {
        let mut args = env::args().skip(1);
        let scenario = match args.next().as_deref() {
            None | Some("approved") => Self::Approved,
            Some("requires_approval") => Self::RequiresApproval,
            Some("blocked_allowlist") => Self::BlockedAllowlist,
            Some(value) => {
                return Err(format!(
                    "unsupported scenario '{value}'; expected approved, requires_approval or blocked_allowlist"
                ));
            }
        };
        if args.next().is_some() {
            return Err("expected at most one scenario argument".to_owned());
        }
        Ok(scenario)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::RequiresApproval => "requires_approval",
            Self::BlockedAllowlist => "blocked_allowlist",
        }
    }

    fn artifact_path(self) -> &'static str {
        match self {
            Self::Approved => APPROVED_PROOF_ARTIFACT,
            Self::RequiresApproval => REQUIRES_APPROVAL_PROOF_ARTIFACT,
            Self::BlockedAllowlist => BLOCKED_ALLOWLIST_PROOF_ARTIFACT,
        }
    }

    fn expected_decision(self) -> DecisionStatus {
        match self {
            Self::Approved => DecisionStatus::Approved,
            Self::RequiresApproval => DecisionStatus::RequiresApproval,
            Self::BlockedAllowlist => DecisionStatus::Blocked,
        }
    }

    fn expected_exit(self) -> ExitCode {
        match self {
            Self::Approved | Self::RequiresApproval => ExitCode::Passed,
            Self::BlockedAllowlist => ExitCode::Allowlist,
        }
    }

    fn expected_reason(self) -> ReasonCode {
        match self {
            Self::Approved => ReasonCode::Passed,
            Self::RequiresApproval => ReasonCode::ApprovalThreshold,
            Self::BlockedAllowlist => ReasonCode::Allowlist,
        }
    }

    fn expected_host_next_step(self) -> VerifiedNextStep {
        match self {
            Self::Approved => VerifiedNextStep::EligibleForSeparateApprovalFlow,
            Self::RequiresApproval => VerifiedNextStep::RequiresApproval,
            Self::BlockedAllowlist => VerifiedNextStep::Blocked,
        }
    }

    fn expected_contract_next_step(self) -> ContractNextStep {
        match self {
            Self::Approved => ContractNextStep::EligibleForSeparateApprovalFlow,
            Self::RequiresApproval => ContractNextStep::RequiresApproval,
            Self::BlockedAllowlist => ContractNextStep::Blocked,
        }
    }

    fn next_step_name(self) -> &'static str {
        match self {
            Self::Approved => "eligible_for_separate_approval_flow",
            Self::RequiresApproval => "requires_approval",
            Self::BlockedAllowlist => "blocked",
        }
    }

    fn exit_code_number(self) -> u8 {
        match self.expected_exit() {
            ExitCode::Passed => 0,
            ExitCode::Allowlist => 3,
            ExitCode::ContractPolicy => 4,
            ExitCode::IntentSafety => 5,
        }
    }
}

#[derive(Debug, Serialize)]
struct StellarProofArtifact {
    schema_version: u32,
    seal_hex: String,
    image_id_hex: String,
    journal_hex: String,
    journal_digest_hex: String,
}

impl StellarProofArtifact {
    fn from_receipt(receipt: &Receipt, image_id: Digest32) -> Result<Self, String> {
        let seal = encode_stellar_seal(receipt)?;
        if seal.len() <= 4 {
            return Err("Groth16 seal must contain a routing selector and proof".to_owned());
        }
        let journal_digest: Digest32 = Sha256::digest(&receipt.journal.bytes).into();

        Ok(Self {
            schema_version: 1,
            seal_hex: hex::encode(seal),
            image_id_hex: hex::encode(image_id),
            journal_hex: hex::encode(&receipt.journal.bytes),
            journal_digest_hex: hex::encode(journal_digest),
        })
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }
}

fn encode_stellar_seal(receipt: &Receipt) -> Result<Vec<u8>, String> {
    let InnerReceipt::Groth16(groth16) = &receipt.inner else {
        return Err("Stellar verifier requires a genuine Groth16 receipt".to_owned());
    };

    let selector = &groth16.verifier_parameters.as_bytes()[..4];
    let mut encoded = Vec::with_capacity(selector.len() + groth16.seal.len());
    encoded.extend_from_slice(selector);
    encoded.extend_from_slice(groth16.seal.as_ref());
    Ok(encoded)
}

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

fn proof_input(scenario: Scenario, evaluator_image_id: Digest32) -> ProofInput {
    let (policy_version, commitment_salt, allowed_contract, approval_threshold_minor, audit_nonce) =
        match scenario {
            Scenario::Approved => (7, [0x55; 32], CONTRACT, 600_000_000, [0x22; 32]),
            Scenario::RequiresApproval => (8, [0x56; 32], CONTRACT, 500_000_000, [0x23; 32]),
            Scenario::BlockedAllowlist => {
                (9, [0x57; 32], BLOCKED_CONTRACT, 600_000_000, [0x24; 32])
            }
        };

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
            policy_version,
            commitment_salt,
            allowed_contracts: vec![allowed_contract.to_owned()],
            allowed_contract_functions: vec![format!("{allowed_contract}:purchase_credits")],
            allowed_assets: vec!["USDC".to_owned()],
            allowed_recipients: vec![RECIPIENT.to_owned()],
            max_amount_minor: 1_000_000_000,
            approval_threshold_minor,
            min_intent_confidence_bps: 9_000,
        },
        audit_nonce,
    }
}

fn main() {
    let scenario = Scenario::from_args().unwrap_or_else(|error| panic!("{error}"));
    let method_id = Digest::from(NEUROCHAIN_ZK_RISC0_GUEST_ID);
    let evaluator_image_id = digest_bytes(method_id);
    let input = proof_input(scenario, evaluator_image_id);
    let env = ExecutorEnv::builder()
        .write(&input)
        .expect("proof input serialization must succeed")
        .build()
        .expect("executor environment must build");

    let receipt = default_prover()
        .prove_with_opts(env, NEUROCHAIN_ZK_RISC0_GUEST_ELF, &ProverOpts::groth16())
        .expect("RISC Zero Groth16 proving must succeed")
        .receipt;
    receipt
        .verify(method_id)
        .expect("generated receipt must verify against the guest image id");
    let receipt_bytes =
        bincode::serialize(&receipt).expect("genuine receipt serialization must succeed");
    let stellar_artifact = StellarProofArtifact::from_receipt(&receipt, evaluator_image_id)
        .expect("Stellar Groth16 proof artifact must encode");
    stellar_artifact
        .write(Path::new(scenario.artifact_path()))
        .expect("Stellar proof artifact must be written under target");

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
        scenario.expected_decision()
    );
    assert_eq!(host_verified.journal.exit_code, scenario.expected_exit());
    assert_eq!(
        host_verified.journal.reason_code,
        scenario.expected_reason()
    );
    assert_eq!(
        host_verified.next_step(),
        scenario.expected_host_next_step()
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
        scenario.expected_contract_next_step()
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
    println!("decision={}", scenario.name());
    println!("exit_code={}", scenario.exit_code_number());
    println!("next_step={}", scenario.next_step_name());
    println!("replay=blocked_exit_4");
    println!("proof_kind=groth16");
    println!("stellar_artifact={}", scenario.artifact_path());
}
