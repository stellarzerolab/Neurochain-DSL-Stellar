use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

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
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha2Digest, Sha256};

const CONTRACT: &str = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const BLOCKED_CONTRACT: &str = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
const RECIPIENT: &str = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const APPROVED_PROOF_ARTIFACT: &str = "target/neurochain-zk-stellar-proof.json";
const REQUIRES_APPROVAL_PROOF_ARTIFACT: &str =
    "target/neurochain-zk-stellar-proof-requires-approval.json";
const BLOCKED_ALLOWLIST_PROOF_ARTIFACT: &str =
    "target/neurochain-zk-stellar-proof-blocked-allowlist.json";
const CUSTOM_PROOF_ARTIFACT: &str = "target/neurochain-zk-stellar-proof-custom.json";
const MAX_PRIVATE_INPUT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateProofInputDocument {
    action_plan: ActionPlanDocument,
    private_policy: PrivatePolicyDocument,
    audit_nonce_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionPlanDocument {
    schema_version: u32,
    intent_label: String,
    action_kind: String,
    contract_id: String,
    function: String,
    args: Vec<TypedArgDocument>,
    intent_confidence_bps: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedArgDocument {
    name: String,
    #[serde(rename = "type")]
    value_type: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivatePolicyDocument {
    schema_version: u32,
    policy_version: u32,
    commitment_salt_hex: String,
    allowed_contracts: Vec<String>,
    allowed_contract_functions: Vec<String>,
    allowed_assets: Vec<String>,
    allowed_recipients: Vec<String>,
    max_amount_minor: u64,
    approval_threshold_minor: u64,
    min_intent_confidence_bps: u16,
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Approved,
    RequiresApproval,
    BlockedAllowlist,
}

impl Scenario {
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
}

#[derive(Debug)]
enum HostRun {
    Scenario(Scenario),
    PrivateInput { input: PathBuf, output: PathBuf },
    CheckPrivateInput { input: PathBuf },
}

impl HostRun {
    fn from_args() -> Result<Self, String> {
        let args = env::args().skip(1).collect::<Vec<_>>();
        if args.is_empty() {
            return Ok(Self::Scenario(Scenario::Approved));
        }
        if args[0] == "--check-input" {
            if args.len() != 2 {
                return Err("input validation requires --check-input <private.json>".to_owned());
            }
            return Ok(Self::CheckPrivateInput {
                input: PathBuf::from(&args[1]),
            });
        }
        if args[0] != "--input" {
            if args.len() != 1 {
                return Err(
                    "use one scenario, or --input <private.json> [--output <public.json>]"
                        .to_owned(),
                );
            }
            let scenario = match args[0].as_str() {
                "approved" => Scenario::Approved,
                "requires_approval" => Scenario::RequiresApproval,
                "blocked_allowlist" => Scenario::BlockedAllowlist,
                value => {
                    return Err(format!(
                        "unsupported scenario '{value}'; expected approved, requires_approval, blocked_allowlist, or --input"
                    ));
                }
            };
            return Ok(Self::Scenario(scenario));
        }
        if args.len() != 2 && args.len() != 4 {
            return Err(
                "custom proving requires --input <private.json> [--output <public.json>]"
                    .to_owned(),
            );
        }
        if args.len() == 4 && args[2] != "--output" {
            return Err("expected --output after the private input path".to_owned());
        }
        let input = PathBuf::from(&args[1]);
        let output = if args.len() == 4 {
            PathBuf::from(&args[3])
        } else {
            PathBuf::from(CUSTOM_PROOF_ARTIFACT)
        };
        if paths_resolve_equal(&input, &output)? {
            return Err(
                "private input and public proof output must use different files".to_owned(),
            );
        }
        Ok(Self::PrivateInput { input, output })
    }
}

fn normalized_absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|error| error.to_string());
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn paths_resolve_equal(left: &Path, right: &Path) -> Result<bool, String> {
    Ok(normalized_absolute_path(left)? == normalized_absolute_path(right)?)
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

fn decode_digest(value: &str, field: &str) -> Result<Digest32, String> {
    let bytes = hex::decode(value).map_err(|_| format!("{field} must be lowercase hex"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{field} must contain exactly 32 bytes"))
}

fn typed_value(document: TypedArgDocument) -> Result<OwnedTypedValue, String> {
    match document.value_type.as_str() {
        "address" => Ok(OwnedTypedValue::Address(document.value)),
        "bytes" => hex::decode(&document.value)
            .map(OwnedTypedValue::Bytes)
            .map_err(|_| format!("argument `{}` has invalid bytes hex", document.name)),
        "symbol" => Ok(OwnedTypedValue::Symbol(document.value)),
        "u64" => document
            .value
            .parse::<u64>()
            .map(OwnedTypedValue::U64)
            .map_err(|_| format!("argument `{}` has invalid u64 value", document.name)),
        value_type => Err(format!(
            "argument `{}` has unsupported type `{value_type}`",
            document.name
        )),
    }
}

fn load_private_input(path: &Path, evaluator_image_id: Digest32) -> Result<ProofInput, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("read private input metadata: {error}"))?;
    if !metadata.is_file() {
        return Err("private input must be a regular file".to_owned());
    }
    if metadata.len() > MAX_PRIVATE_INPUT_BYTES {
        return Err(format!(
            "private input exceeds the {MAX_PRIVATE_INPUT_BYTES} byte limit"
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("read private input: {error}"))?;
    let document: PrivateProofInputDocument =
        serde_json::from_slice(&bytes).map_err(|error| format!("decode private input: {error}"))?;
    let args = document
        .action_plan
        .args
        .into_iter()
        .map(|arg| {
            let name = arg.name.clone();
            typed_value(arg).map(|value| OwnedTypedArg { name, value })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProofInput {
        evaluator_image_id,
        action_plan: OwnedActionPlan {
            schema_version: document.action_plan.schema_version,
            intent_label: document.action_plan.intent_label,
            action_kind: document.action_plan.action_kind,
            contract_id: document.action_plan.contract_id,
            function: document.action_plan.function,
            args,
            intent_confidence_bps: document.action_plan.intent_confidence_bps,
        },
        private_policy: OwnedPrivatePolicy {
            schema_version: document.private_policy.schema_version,
            policy_version: document.private_policy.policy_version,
            commitment_salt: decode_digest(
                &document.private_policy.commitment_salt_hex,
                "private_policy.commitment_salt_hex",
            )?,
            allowed_contracts: document.private_policy.allowed_contracts,
            allowed_contract_functions: document.private_policy.allowed_contract_functions,
            allowed_assets: document.private_policy.allowed_assets,
            allowed_recipients: document.private_policy.allowed_recipients,
            max_amount_minor: document.private_policy.max_amount_minor,
            approval_threshold_minor: document.private_policy.approval_threshold_minor,
            min_intent_confidence_bps: document.private_policy.min_intent_confidence_bps,
        },
        audit_nonce: decode_digest(&document.audit_nonce_hex, "audit_nonce_hex")?,
    })
}

fn decision_name(value: DecisionStatus) -> &'static str {
    match value {
        DecisionStatus::Approved => "approved",
        DecisionStatus::Blocked => "blocked",
        DecisionStatus::RequiresApproval => "requires_approval",
    }
}

fn exit_code_number(value: ExitCode) -> u8 {
    match value {
        ExitCode::Passed => 0,
        ExitCode::Allowlist => 3,
        ExitCode::ContractPolicy => 4,
        ExitCode::IntentSafety => 5,
    }
}

fn next_step_name(value: VerifiedNextStep) -> &'static str {
    match value {
        VerifiedNextStep::EligibleForSeparateApprovalFlow => "eligible_for_separate_approval_flow",
        VerifiedNextStep::RequiresApproval => "requires_approval",
        VerifiedNextStep::Blocked => "blocked",
    }
}

fn main() {
    let run = HostRun::from_args().unwrap_or_else(|error| panic!("{error}"));
    let method_id = Digest::from(NEUROCHAIN_ZK_RISC0_GUEST_ID);
    let evaluator_image_id = digest_bytes(method_id);
    if let HostRun::CheckPrivateInput { input } = &run {
        load_private_input(input, evaluator_image_id).unwrap_or_else(|error| panic!("{error}"));
        println!("private_input_valid=true");
        println!("proof_generated=false");
        return;
    }
    let (input, artifact_path, scenario) = match run {
        HostRun::Scenario(scenario) => (
            proof_input(scenario, evaluator_image_id),
            PathBuf::from(scenario.artifact_path()),
            Some(scenario),
        ),
        HostRun::PrivateInput { input, output } => (
            load_private_input(&input, evaluator_image_id)
                .unwrap_or_else(|error| panic!("{error}")),
            output,
            None,
        ),
        HostRun::CheckPrivateInput { .. } => unreachable!("handled before proving"),
    };
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
        .write(&artifact_path)
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
    if let Some(scenario) = scenario {
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
    }

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
    if let Some(scenario) = scenario {
        assert_eq!(
            contract_verified.next_step(),
            scenario.expected_contract_next_step()
        );
    }

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
    println!(
        "decision={}",
        decision_name(host_verified.journal.decision_status)
    );
    println!(
        "exit_code={}",
        exit_code_number(host_verified.journal.exit_code)
    );
    println!("next_step={}", next_step_name(host_verified.next_step()));
    println!("replay=blocked_exit_4");
    println!("proof_kind=groth16");
    println!("stellar_artifact={}", artifact_path.display());
}
