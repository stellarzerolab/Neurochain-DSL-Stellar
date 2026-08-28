use neurochain::{
    product_local_reference_path::run_product_local_reference_path,
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate, BazaarCatalogKey},
    x402_bazaar_paid_call::{
        BazaarPaidCallAccessDecision, BazaarPaidCallAccessGate, BazaarPaidCallBinding,
    },
    x402_local_reference_path::{
        X402LocalAccessState, X402LocalAccessStatePort, X402LocalEvaluationPort,
        X402LocalReferencePathRequest,
    },
    x402_service_boundary::{X402ServiceEvaluationRequest, X402ServiceEvaluationResponse},
    zk_attestation::{ZkAttestationViewRequest, ZkProofArtifact, ZkTypedActionPlan},
};
use serde::Deserialize;
use serde_json::{json, Value};

const MANIFEST_JSON: &str = include_str!("product_local_quickstart/manifest.json");
const CATALOG_JSON: &str = include_str!("x402_bazaar_catalog/mcp_tool.json");
const ZK_ACTION_PLAN_JSON: &str =
    include_str!("../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickstartManifest {
    schema_version: u32,
    catalog_fixture: String,
    zk_action_plan_fixture: String,
    scenarios: Vec<QuickstartScenario>,
    authority: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickstartScenario {
    name: String,
    request_fixture: String,
    evaluation_response_fixture: String,
    zk_proof_fixture: String,
    expected_outcome: String,
    expected_capability_code: String,
    expected_access_consumed: bool,
    expected_proof_reason_code: String,
}

struct SettledAccess;

impl X402LocalAccessStatePort for SettledAccess {
    fn inspect_access(&self, _resource_key: &BazaarCatalogKey) -> X402LocalAccessState {
        X402LocalAccessState::SettledAccessReady
    }
}

struct FixtureEvaluation {
    response: Option<X402ServiceEvaluationResponse>,
}

impl X402LocalEvaluationPort for FixtureEvaluation {
    fn plan_and_evaluate(
        &mut self,
        _request: &X402ServiceEvaluationRequest,
    ) -> Result<X402ServiceEvaluationResponse, String> {
        self.response
            .take()
            .ok_or_else(|| "product quickstart evaluation fixture was already consumed".to_string())
    }
}

#[derive(Default)]
struct FixtureCapabilityGate {
    calls: usize,
}

impl BazaarPaidCallAccessGate for FixtureCapabilityGate {
    fn consume_settled_access(
        &mut self,
        _binding: &BazaarPaidCallBinding,
    ) -> BazaarPaidCallAccessDecision {
        self.calls += 1;
        BazaarPaidCallAccessDecision::Authorized
    }
}

fn parse_json<T: serde::de::DeserializeOwned>(label: &str, raw: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|error| format!("parse {label}: {error}"))
}

fn local_fixture(name: &str) -> Result<&'static str, String> {
    match name {
        "approved_request.json" => Ok(include_str!(
            "product_local_quickstart/approved_request.json"
        )),
        "approved_evaluation_response.json" => Ok(include_str!(
            "product_local_quickstart/approved_evaluation_response.json"
        )),
        "approval_required_request.json" => Ok(include_str!(
            "product_local_quickstart/approval_required_request.json"
        )),
        "approval_required_evaluation_response.json" => Ok(include_str!(
            "product_local_quickstart/approval_required_evaluation_response.json"
        )),
        "blocked_request.json" => Ok(include_str!(
            "product_local_quickstart/blocked_request.json"
        )),
        "blocked_evaluation_response.json" => Ok(include_str!(
            "product_local_quickstart/blocked_evaluation_response.json"
        )),
        _ => Err(format!("unsupported product quickstart fixture: {name}")),
    }
}

fn proof_fixture(name: &str) -> Result<&'static str, String> {
    match name {
        "../../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json" => Ok(
            include_str!("../hackathons/stellar-real-world-zk/fixtures/groth16_approved.json"),
        ),
        "../../hackathons/stellar-real-world-zk/fixtures/groth16_requires_approval.json" => {
            Ok(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/groth16_requires_approval.json"
            ))
        }
        "../../hackathons/stellar-real-world-zk/fixtures/groth16_blocked_exit_3.json" => {
            Ok(include_str!(
                "../hackathons/stellar-real-world-zk/fixtures/groth16_blocked_exit_3.json"
            ))
        }
        _ => Err(format!("unsupported product ZK proof fixture: {name}")),
    }
}

fn all_false_authority(
    label: &str,
    authority: &Value,
    expected_fields: usize,
) -> Result<(), String> {
    let fields = authority
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))?;
    if fields.len() != expected_fields || fields.values().any(|value| value != &Value::Bool(false))
    {
        return Err(format!(
            "{label} must contain exactly {expected_fields} all-false authority fields"
        ));
    }
    Ok(())
}

fn local_catalog() -> Result<BazaarCatalog, String> {
    let candidate: BazaarCatalogCandidate = parse_json("catalog fixture", CATALOG_JSON)?;
    let mut catalog = BazaarCatalog::default();
    catalog
        .insert(candidate, 1_723_000_001)
        .map_err(|error| format!("insert catalog fixture: {error}"))?;
    Ok(catalog)
}

pub fn quickstart_report() -> Result<Value, String> {
    let manifest: QuickstartManifest = parse_json("product quickstart manifest", MANIFEST_JSON)?;
    if manifest.schema_version != 1
        || manifest.catalog_fixture != "../x402_bazaar_catalog/mcp_tool.json"
        || manifest.zk_action_plan_fixture
            != "../../hackathons/stellar-real-world-zk/fixtures/typed_action_plan.json"
    {
        return Err("product quickstart manifest references drifted".to_string());
    }
    if manifest.scenarios.len() != 3
        || manifest.scenarios[0].name != "approved"
        || manifest.scenarios[1].name != "requires_approval"
        || manifest.scenarios[2].name != "blocked"
    {
        return Err(
            "product quickstart manifest must contain approved, requires_approval, then blocked"
                .to_string(),
        );
    }
    all_false_authority("manifest authority", &manifest.authority, 12)?;

    let zk_action_plan: ZkTypedActionPlan =
        parse_json("product ZK typed ActionPlan", ZK_ACTION_PLAN_JSON)?;
    let catalog = local_catalog()?;
    let access = SettledAccess;
    let mut reports = Vec::with_capacity(manifest.scenarios.len());

    for scenario in manifest.scenarios {
        let request: X402LocalReferencePathRequest = parse_json(
            &scenario.request_fixture,
            local_fixture(&scenario.request_fixture)?,
        )?;
        let response: X402ServiceEvaluationResponse = parse_json(
            &scenario.evaluation_response_fixture,
            local_fixture(&scenario.evaluation_response_fixture)?,
        )?;
        let proof: ZkProofArtifact = parse_json(
            &scenario.zk_proof_fixture,
            proof_fixture(&scenario.zk_proof_fixture)?,
        )?;
        let mut evaluation = FixtureEvaluation {
            response: Some(response),
        };
        let mut capability = FixtureCapabilityGate::default();
        let result = run_product_local_reference_path(
            &catalog,
            &access,
            &mut evaluation,
            Some(&mut capability),
            request,
            ZkAttestationViewRequest {
                action_plan: zk_action_plan.clone(),
                proof,
            },
        )?;

        let outcome = serde_json::to_value(result.reference_path.outcome)
            .map_err(|error| format!("serialize outcome: {error}"))?;
        if outcome != scenario.expected_outcome
            || result.reference_path.capability_gate.code != scenario.expected_capability_code
            || result.reference_path.capability_gate.access_consumed
                != scenario.expected_access_consumed
            || result.zk_evidence.proof_reason_code != scenario.expected_proof_reason_code
        {
            return Err(format!(
                "{} scenario does not match its versioned expectations",
                scenario.name
            ));
        }

        let authority = serde_json::to_value(result.authority)
            .map_err(|error| format!("serialize product authority: {error}"))?;
        let proof_authority = serde_json::to_value(result.zk_evidence.authority)
            .map_err(|error| format!("serialize proof authority: {error}"))?;
        let reference_authority = serde_json::to_value(result.reference_path.authority)
            .map_err(|error| format!("serialize x402 reference authority: {error}"))?;
        all_false_authority(&format!("{} authority", scenario.name), &authority, 12)?;
        all_false_authority(
            &format!("{} proof authority", scenario.name),
            &proof_authority,
            12,
        )?;
        all_false_authority(
            &format!("{} x402 reference authority", scenario.name),
            &reference_authority,
            11,
        )?;
        if authority != manifest.authority || proof_authority != manifest.authority {
            return Err(format!(
                "{} authority drifted from the versioned manifest",
                scenario.name
            ));
        }

        let expected_gate_calls = usize::from(scenario.expected_access_consumed);
        if capability.calls != expected_gate_calls {
            return Err(format!(
                "{} capability gate calls: expected {expected_gate_calls}, got {}",
                scenario.name, capability.calls
            ));
        }

        reports.push(json!({
            "actionPlanHash": result.reference_path.evaluation.action_plan_hash,
            "authority": authority,
            "capability": {
                "accessConsumed": result.reference_path.capability_gate.access_consumed,
                "code": result.reference_path.capability_gate.code,
                "gateCalls": capability.calls,
                "serviceCallAllowed": result.reference_path.capability_gate.service_call_allowed,
                "serviceDispatchAllowed": result.reference_path.capability_gate.service_dispatch_allowed,
            },
            "decision": result.reference_path.evaluation.decision,
            "evaluationReasonCode": result.reference_path.evaluation.reason_code,
            "exitCode": result.reference_path.evaluation.exit_code,
            "name": scenario.name,
            "outcome": outcome,
            "zkEvidence": {
                "actionPlanProjectionValidated": result.zk_evidence.action_plan_projection_validated,
                "artifactPresent": result.zk_evidence.artifact_present,
                "authority": proof_authority,
                "cryptographicallyVerified": result.zk_evidence.cryptographically_verified,
                "localBinding": result.zk_evidence.local_binding,
                "privatePolicyRevealed": result.zk_evidence.private_policy_revealed,
                "proofActionPlanHash": result.zk_evidence.proof_action_plan_hash,
                "proofKind": result.zk_evidence.proof_kind,
                "proofReasonCode": result.zk_evidence.proof_reason_code,
                "source": result.zk_evidence.source,
                "stellarVerificationRequired": result.zk_evidence.stellar_verification_required,
            },
        }));
    }

    Ok(json!({
        "authorityBoundary": manifest.authority,
        "credentialRequired": false,
        "listenerRequired": false,
        "networkRequired": false,
        "offline": true,
        "path": [
            "bazaar_discovery",
            "x402_access_state",
            "typed_action_plan",
            "deterministic_policy",
            "optional_zk_proof_artifact",
            "local_zk_binding_verify",
            "separate_exact_capability_gate"
        ],
        "scenarios": reports,
        "schemaVersion": 1,
        "status": "product_local_reference_ready",
        "verificationBoundary": "local_binding_only_cryptographic_stellar_verify_not_run"
    }))
}

#[cfg(not(test))]
fn main() {
    match quickstart_report() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("product local quickstart failed: serialize report: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("product local quickstart failed: {error}");
            std::process::exit(1);
        }
    }
}
