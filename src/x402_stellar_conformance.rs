use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const X402_STELLAR_CONFORMANCE_SCHEMA_VERSION: u32 = 1;
pub const X402_PROTOCOL_VERSION: u32 = 2;
pub const X402_SOURCE_SNAPSHOT_DATE: &str = "2026-08-22";
const MAX_PLAN_BYTES: usize = 128 * 1024;
const MAX_REASON_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum X402StellarNetwork {
    #[serde(rename = "stellar:testnet")]
    Testnet,
    #[serde(rename = "stellar:pubnet")]
    Pubnet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402ConformanceScheme {
    Exact,
    Upto,
    All,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402ConformanceMode {
    OfflineFixture,
    CanonicalClientLive,
    UpstreamContribution,
    ExternalReview,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402ConformanceStatus {
    Ready,
    ApprovalBlocked,
    UpstreamBlocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402ConformanceExpected {
    Advertise,
    Accept,
    Reject,
    Review,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum X402ConformanceEvidence {
    WireFixture,
    PackageE2e,
    TransactionHash,
    RejectionReason,
    SpecRevision,
    AuthEntryMatrix,
    SimulationAssertion,
    MetricsContract,
    AuditContract,
    ReviewReport,
    UptoSpecPr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct X402ConformanceSource {
    pub id: String,
    pub url: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct X402ConformanceSourceSnapshot {
    pub checked_at: String,
    pub protocol_version: u32,
    pub exact_stellar_spec_present: bool,
    pub upto_stellar_spec_present: bool,
    pub networks: Vec<X402StellarNetwork>,
    pub sources: Vec<X402ConformanceSource>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402VerifySettleOwner {
    UpstreamPackage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402PackageSelectionStatus {
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct X402ConformanceDependencyBoundary {
    pub package_name: String,
    pub license: String,
    pub verify_settle_owner: X402VerifySettleOwner,
    pub package_selection_status: X402PackageSelectionStatus,
    pub package_installed: bool,
    pub runtime_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct X402ConformanceCase {
    pub id: String,
    pub scheme: X402ConformanceScheme,
    pub networks: Vec<X402StellarNetwork>,
    pub mode: X402ConformanceMode,
    pub status: X402ConformanceStatus,
    pub expected: X402ConformanceExpected,
    pub evidence: Vec<X402ConformanceEvidence>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct X402StellarConformancePlan {
    pub schema_version: u32,
    pub source_snapshot: X402ConformanceSourceSnapshot,
    pub dependency_boundary: X402ConformanceDependencyBoundary,
    pub cases: Vec<X402ConformanceCase>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct X402ConformanceAuthority {
    payment_verification_allowed: bool,
    payment_settlement_allowed: bool,
    wallet_signing_allowed: bool,
    network_access_allowed: bool,
    service_dispatch_allowed: bool,
    rpc_submit_allowed: bool,
    action_plan_submit_allowed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct X402ConformancePlanSummary {
    pub total_cases: usize,
    pub offline_ready_cases: usize,
    pub approval_blocked_cases: Vec<String>,
    pub upstream_blocked_cases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct X402ConformancePlanReport {
    pub schema_version: u32,
    pub ok: bool,
    pub code: String,
    pub reason: String,
    pub authority: X402ConformanceAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<X402ConformancePlanSummary>,
}

impl X402ConformancePlanReport {
    fn ready(plan: &X402StellarConformancePlan) -> Self {
        let approval_blocked_cases = plan
            .cases
            .iter()
            .filter(|case| case.status == X402ConformanceStatus::ApprovalBlocked)
            .map(|case| case.id.clone())
            .collect::<Vec<_>>();
        let upstream_blocked_cases = plan
            .cases
            .iter()
            .filter(|case| case.status == X402ConformanceStatus::UpstreamBlocked)
            .map(|case| case.id.clone())
            .collect::<Vec<_>>();
        Self {
            schema_version: X402_STELLAR_CONFORMANCE_SCHEMA_VERSION,
            ok: true,
            code: "conformance_plan_ready".to_string(),
            reason: "Offline conformance coverage is complete; this does not claim package, network, or settlement conformance.".to_string(),
            authority: X402ConformanceAuthority::default(),
            data: Some(X402ConformancePlanSummary {
                total_cases: plan.cases.len(),
                offline_ready_cases: plan
                    .cases
                    .iter()
                    .filter(|case| case.status == X402ConformanceStatus::Ready)
                    .count(),
                approval_blocked_cases,
                upstream_blocked_cases,
            }),
        }
    }

    fn rejected(code: &str, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        debug_assert!(!code.is_empty());
        debug_assert!(!reason.is_empty());
        Self {
            schema_version: X402_STELLAR_CONFORMANCE_SCHEMA_VERSION,
            ok: false,
            code: code.to_string(),
            reason,
            authority: X402ConformanceAuthority::default(),
            data: None,
        }
    }
}

#[derive(Clone, Copy)]
struct RequiredCase {
    id: &'static str,
    scheme: X402ConformanceScheme,
    mode: X402ConformanceMode,
    status: X402ConformanceStatus,
    expected: X402ConformanceExpected,
    evidence: &'static [X402ConformanceEvidence],
}

const WIRE: &[X402ConformanceEvidence] = &[X402ConformanceEvidence::WireFixture];
const WIRE_REASON: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::WireFixture,
    X402ConformanceEvidence::RejectionReason,
];
const LIVE_ACCEPT: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::PackageE2e,
    X402ConformanceEvidence::TransactionHash,
];
const LIVE_REJECT: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::PackageE2e,
    X402ConformanceEvidence::RejectionReason,
];
const LIVE_AUTH: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::PackageE2e,
    X402ConformanceEvidence::TransactionHash,
    X402ConformanceEvidence::AuthEntryMatrix,
];
const AUTH_REASON: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::WireFixture,
    X402ConformanceEvidence::AuthEntryMatrix,
    X402ConformanceEvidence::RejectionReason,
];
const SIMULATION_REASON: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::PackageE2e,
    X402ConformanceEvidence::SimulationAssertion,
    X402ConformanceEvidence::RejectionReason,
];
const UPTO: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::SpecRevision,
    X402ConformanceEvidence::UptoSpecPr,
];
const OPERATIONS: &[X402ConformanceEvidence] = &[
    X402ConformanceEvidence::MetricsContract,
    X402ConformanceEvidence::AuditContract,
];
const REVIEW: &[X402ConformanceEvidence] = &[X402ConformanceEvidence::ReviewReport];
const SPEC: &[X402ConformanceEvidence] = &[X402ConformanceEvidence::SpecRevision];

const REQUIRED_CASES: &[RequiredCase] = &[
    required(
        "standard_surface",
        X402ConformanceScheme::All,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Advertise,
        WIRE,
    ),
    required(
        "supported_exact_both_networks",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Advertise,
        WIRE,
    ),
    required(
        "supported_are_fees_sponsored",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Advertise,
        WIRE,
    ),
    required(
        "wire_v2_payload_transaction",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Accept,
        WIRE,
    ),
    required(
        "exact_canonical_client_e2e",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Accept,
        LIVE_ACCEPT,
    ),
    required(
        "exact_keypair_auth",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Accept,
        LIVE_AUTH,
    ),
    required(
        "exact_custom_check_auth",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Accept,
        LIVE_AUTH,
    ),
    required(
        "exact_sep41_seven_decimals",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Accept,
        LIVE_ACCEPT,
    ),
    required(
        "exact_tampered_signature_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        WIRE_REASON,
    ),
    required(
        "exact_asset_mismatch_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        WIRE_REASON,
    ),
    required(
        "exact_amount_mismatch_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        WIRE_REASON,
    ),
    required(
        "exact_recipient_mismatch_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        WIRE_REASON,
    ),
    required(
        "exact_expired_auth_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        AUTH_REASON,
    ),
    required(
        "exact_replay_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Reject,
        LIVE_REJECT,
    ),
    required(
        "exact_auth_structure_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        AUTH_REASON,
    ),
    required(
        "exact_facilitator_non_custodial",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        AUTH_REASON,
    ),
    required(
        "exact_simulation_balance_change_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Reject,
        SIMULATION_REASON,
    ),
    required(
        "exact_missing_trustline_reject",
        X402ConformanceScheme::Exact,
        X402ConformanceMode::CanonicalClientLive,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Reject,
        LIVE_REJECT,
    ),
    required(
        "rejections_non_null_reason",
        X402ConformanceScheme::All,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Reject,
        WIRE_REASON,
    ),
    required(
        "upto_stellar_upstream_spec",
        X402ConformanceScheme::Upto,
        X402ConformanceMode::UpstreamContribution,
        X402ConformanceStatus::UpstreamBlocked,
        X402ConformanceExpected::Review,
        UPTO,
    ),
    required(
        "upto_single_use_time_bound_cap",
        X402ConformanceScheme::Upto,
        X402ConformanceMode::UpstreamContribution,
        X402ConformanceStatus::UpstreamBlocked,
        X402ConformanceExpected::Review,
        UPTO,
    ),
    required(
        "spec_drift_gate",
        X402ConformanceScheme::All,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Review,
        SPEC,
    ),
    required(
        "observability_and_audit",
        X402ConformanceScheme::All,
        X402ConformanceMode::OfflineFixture,
        X402ConformanceStatus::Ready,
        X402ConformanceExpected::Review,
        OPERATIONS,
    ),
    required(
        "third_party_security_review",
        X402ConformanceScheme::All,
        X402ConformanceMode::ExternalReview,
        X402ConformanceStatus::ApprovalBlocked,
        X402ConformanceExpected::Review,
        REVIEW,
    ),
];

const fn required(
    id: &'static str,
    scheme: X402ConformanceScheme,
    mode: X402ConformanceMode,
    status: X402ConformanceStatus,
    expected: X402ConformanceExpected,
    evidence: &'static [X402ConformanceEvidence],
) -> RequiredCase {
    RequiredCase {
        id,
        scheme,
        mode,
        status,
        expected,
        evidence,
    }
}

const REQUIRED_SOURCES: &[(&str, &str, &str)] = &[
    (
        "scf_rfp",
        "https://github.com/stellar/scf-handbook/blob/main/scf-awards/build-award/rfp-track.md",
        "main checked 2026-08-22",
    ),
    (
        "stellar_x402_docs",
        "https://developers.stellar.org/docs/build/agentic-payments/x402",
        "page updated 2026-08-22",
    ),
    (
        "exact_stellar_spec",
        "https://github.com/x402-foundation/x402/blob/main/specs/schemes/exact/scheme_exact_stellar.md",
        "main checked 2026-08-22",
    ),
    (
        "upto_generic_spec",
        "https://github.com/x402-foundation/x402/blob/main/specs/schemes/upto/scheme_upto.md",
        "main checked 2026-08-22",
    ),
    (
        "x402_repo",
        "https://github.com/x402-foundation/x402",
        "main checked 2026-08-22",
    ),
    (
        "stellar_reference_repo",
        "https://github.com/stellar/x402-stellar",
        "main checked 2026-08-22",
    ),
];

pub fn validate_x402_stellar_conformance_plan(
    plan: &X402StellarConformancePlan,
) -> X402ConformancePlanReport {
    let encoded_len = serde_json::to_vec(plan)
        .map(|encoded| encoded.len())
        .unwrap_or(usize::MAX);
    if encoded_len > MAX_PLAN_BYTES {
        return X402ConformancePlanReport::rejected(
            "conformance_plan_too_large",
            "Conformance plan exceeds the 131072-byte offline boundary.",
        );
    }
    if plan.schema_version != X402_STELLAR_CONFORMANCE_SCHEMA_VERSION {
        return X402ConformancePlanReport::rejected(
            "unsupported_conformance_schema_version",
            "Conformance schemaVersion must be 1.",
        );
    }
    if let Err(reason) = validate_source_snapshot(&plan.source_snapshot) {
        return X402ConformancePlanReport::rejected("spec_drift_detected", reason);
    }
    if let Err(reason) = validate_dependency_boundary(&plan.dependency_boundary) {
        return X402ConformancePlanReport::rejected("invalid_dependency_boundary", reason);
    }
    if let Err((code, reason)) = validate_cases(&plan.cases) {
        return X402ConformancePlanReport::rejected(code, reason);
    }
    X402ConformancePlanReport::ready(plan)
}

fn validate_source_snapshot(snapshot: &X402ConformanceSourceSnapshot) -> Result<(), String> {
    if !is_iso_date(&snapshot.checked_at) || snapshot.checked_at != X402_SOURCE_SNAPSHOT_DATE {
        return Err(format!(
            "source snapshot date drifted from {X402_SOURCE_SNAPSHOT_DATE}; refresh all pinned sources together"
        ));
    }
    if snapshot.protocol_version != X402_PROTOCOL_VERSION {
        return Err(format!(
            "x402 protocol version drifted from pinned v{X402_PROTOCOL_VERSION}"
        ));
    }
    if !snapshot.exact_stellar_spec_present {
        return Err("the pinned Stellar exact network specification is missing".to_string());
    }
    if snapshot.upto_stellar_spec_present {
        return Err(
            "a Stellar upto network specification now exists; review and replace the upstream-blocked assumptions"
                .to_string(),
        );
    }
    validate_networks(&snapshot.networks).map_err(|_| {
        "network identifiers drifted from stellar:testnet and stellar:pubnet".to_string()
    })?;

    if snapshot.sources.len() != REQUIRED_SOURCES.len() {
        return Err("source snapshot must contain the complete pinned source set".to_string());
    }
    let mut seen = BTreeSet::new();
    for source in &snapshot.sources {
        if !seen.insert(source.id.as_str()) {
            return Err(format!(
                "source snapshot contains duplicate id {}",
                source.id
            ));
        }
        let Some((_, expected_url, expected_revision)) =
            REQUIRED_SOURCES.iter().find(|(id, _, _)| *id == source.id)
        else {
            return Err(format!("source snapshot contains unknown id {}", source.id));
        };
        if source.url != *expected_url {
            return Err(format!("source URL drifted for {}", source.id));
        }
        if source.revision != *expected_revision {
            return Err(format!("source revision drifted for {}", source.id));
        }
    }
    Ok(())
}

fn validate_dependency_boundary(
    boundary: &X402ConformanceDependencyBoundary,
) -> Result<(), String> {
    if boundary.package_name != "@x402/stellar" {
        return Err("future verify/settle dependency must be @x402/stellar".to_string());
    }
    if boundary.license != "Apache-2.0" {
        return Err("@x402/stellar license snapshot must remain Apache-2.0".to_string());
    }
    if boundary.verify_settle_owner != X402VerifySettleOwner::UpstreamPackage {
        return Err("verify/settle ownership must remain with the upstream package".to_string());
    }
    if boundary.package_installed || boundary.runtime_approved {
        return Err(
            "offline conformance preparation cannot claim an installed package or approved runtime"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_cases(cases: &[X402ConformanceCase]) -> Result<(), (&'static str, String)> {
    let mut seen = BTreeSet::new();
    for case in cases {
        if !seen.insert(case.id.as_str()) {
            return Err((
                "duplicate_conformance_case",
                format!("conformance plan contains duplicate case {}", case.id),
            ));
        }
        let Some(required) = REQUIRED_CASES
            .iter()
            .find(|required| required.id == case.id)
        else {
            return Err((
                "unexpected_conformance_case",
                format!("conformance plan contains unknown case {}", case.id),
            ));
        };
        if case.reason.trim().is_empty() || case.reason.len() > MAX_REASON_BYTES {
            return Err((
                "invalid_conformance_case_reason",
                format!("case {} requires a bounded non-empty reason", case.id),
            ));
        }
        if validate_networks(&case.networks).is_err()
            || case.scheme != required.scheme
            || case.mode != required.mode
            || case.status != required.status
            || case.expected != required.expected
            || !same_evidence(&case.evidence, required.evidence)
        {
            return Err((
                "conformance_case_mismatch",
                format!(
                    "case {} does not match the pinned conformance contract",
                    case.id
                ),
            ));
        }
    }
    if cases.len() != REQUIRED_CASES.len() {
        let missing = REQUIRED_CASES
            .iter()
            .filter(|required| !seen.contains(required.id))
            .map(|required| required.id)
            .collect::<Vec<_>>();
        return Err((
            "missing_conformance_case",
            format!("conformance plan is missing cases: {}", missing.join(", ")),
        ));
    }
    Ok(())
}

fn validate_networks(networks: &[X402StellarNetwork]) -> Result<(), ()> {
    if networks == [X402StellarNetwork::Testnet, X402StellarNetwork::Pubnet] {
        Ok(())
    } else {
        Err(())
    }
}

fn same_evidence(actual: &[X402ConformanceEvidence], expected: &[X402ConformanceEvidence]) -> bool {
    let actual_set = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    actual.len() == actual_set.len()
        && expected.len() == expected_set.len()
        && actual_set == expected_set
}

fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}
