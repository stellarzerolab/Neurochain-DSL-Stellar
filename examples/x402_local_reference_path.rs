use neurochain::{
    x402_bazaar::{BazaarCatalog, BazaarCatalogCandidate, BazaarCatalogKey},
    x402_bazaar_paid_call::{
        BazaarPaidCallAccessDecision, BazaarPaidCallAccessGate, BazaarPaidCallBinding,
    },
    x402_local_reference_path::{
        run_x402_local_reference_path, X402LocalAccessState, X402LocalAccessStatePort,
        X402LocalEvaluationPort, X402LocalReferencePathRequest,
    },
    x402_service_boundary::{X402ServiceEvaluationRequest, X402ServiceEvaluationResponse},
};
use serde::Deserialize;
use serde_json::{json, Value};

const MANIFEST_JSON: &str = include_str!("x402_local_reference_path/manifest.json");
const CATALOG_JSON: &str = include_str!("x402_bazaar_catalog/mcp_tool.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickstartManifest {
    schema_version: u32,
    catalog_fixture: String,
    scenarios: Vec<QuickstartScenario>,
    authority: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickstartScenario {
    name: String,
    request_fixture: String,
    evaluation_response_fixture: String,
    expected_outcome: String,
    expected_capability_code: String,
    expected_access_consumed: bool,
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
            .ok_or_else(|| "quickstart evaluation fixture was already consumed".to_string())
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

fn fixture(name: &str) -> Result<&'static str, String> {
    match name {
        "approved_request.json" => Ok(include_str!(
            "x402_local_reference_path/approved_request.json"
        )),
        "approved_evaluation_response.json" => Ok(include_str!(
            "x402_local_reference_path/approved_evaluation_response.json"
        )),
        "blocked_request.json" => Ok(include_str!(
            "x402_local_reference_path/blocked_request.json"
        )),
        "blocked_evaluation_response.json" => Ok(include_str!(
            "x402_local_reference_path/blocked_evaluation_response.json"
        )),
        _ => Err(format!("unsupported quickstart fixture: {name}")),
    }
}

fn all_false_authority(label: &str, authority: &Value) -> Result<(), String> {
    let fields = authority
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))?;
    if fields.len() != 11 || fields.values().any(|value| value != &Value::Bool(false)) {
        return Err(format!(
            "{label} must contain the exact eleven all-false authority fields"
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
    let manifest: QuickstartManifest = parse_json("quickstart manifest", MANIFEST_JSON)?;
    if manifest.schema_version != 1 {
        return Err("quickstart manifest schema_version must be 1".to_string());
    }
    if manifest.catalog_fixture != "../x402_bazaar_catalog/mcp_tool.json" {
        return Err(
            "quickstart manifest must name the canonical local catalog fixture".to_string(),
        );
    }
    if manifest.scenarios.len() != 2
        || manifest.scenarios[0].name != "approved"
        || manifest.scenarios[1].name != "blocked"
    {
        return Err("quickstart manifest must contain approved then blocked".to_string());
    }
    all_false_authority("manifest authority", &manifest.authority)?;

    let catalog = local_catalog()?;
    let access = SettledAccess;
    let mut reports = Vec::with_capacity(manifest.scenarios.len());

    for scenario in manifest.scenarios {
        let request: X402LocalReferencePathRequest = parse_json(
            &scenario.request_fixture,
            fixture(&scenario.request_fixture)?,
        )?;
        let response: X402ServiceEvaluationResponse = parse_json(
            &scenario.evaluation_response_fixture,
            fixture(&scenario.evaluation_response_fixture)?,
        )?;
        let mut evaluation = FixtureEvaluation {
            response: Some(response),
        };
        let mut capability = FixtureCapabilityGate::default();
        let result = run_x402_local_reference_path(
            &catalog,
            &access,
            &mut evaluation,
            Some(&mut capability),
            request,
        )?;

        let outcome = serde_json::to_value(result.outcome)
            .map_err(|error| format!("serialize outcome: {error}"))?;
        if outcome != scenario.expected_outcome
            || result.capability_gate.code != scenario.expected_capability_code
            || result.capability_gate.access_consumed != scenario.expected_access_consumed
        {
            return Err(format!(
                "{} scenario does not match its versioned expectations",
                scenario.name
            ));
        }

        let authority = serde_json::to_value(result.authority)
            .map_err(|error| format!("serialize authority: {error}"))?;
        all_false_authority(&format!("{} authority", scenario.name), &authority)?;
        if authority != manifest.authority {
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
            "authority": authority,
            "capability": {
                "accessConsumed": result.capability_gate.access_consumed,
                "code": result.capability_gate.code,
                "gateCalls": capability.calls,
                "serviceCallAllowed": result.capability_gate.service_call_allowed,
                "serviceDispatchAllowed": result.capability_gate.service_dispatch_allowed,
            },
            "decision": result.evaluation.decision,
            "exitCode": result.evaluation.exit_code,
            "name": scenario.name,
            "outcome": outcome,
        }));
    }

    Ok(json!({
        "authorityBoundary": manifest.authority,
        "credentialRequired": false,
        "networkRequired": false,
        "offline": true,
        "path": [
            "bazaar_discovery",
            "x402_access_state",
            "typed_action_plan",
            "deterministic_policy",
            "approved_or_blocked",
            "exact_capability_gate"
        ],
        "scenarios": reports,
        "schemaVersion": 1,
        "status": "local_reference_ready"
    }))
}

#[cfg(not(test))]
fn main() {
    match quickstart_report() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("x402 local quickstart failed: serialize report: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("x402 local quickstart failed: {error}");
            std::process::exit(1);
        }
    }
}
