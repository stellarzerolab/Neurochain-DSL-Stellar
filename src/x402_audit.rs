use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::x402_store::{now_unix_secs, X402SettlementRecord};

pub fn write_x402_audit_event(
    logs: &mut Vec<String>,
    event: &str,
    http_status: StatusCode,
    audit_id: &str,
    payment: &Value,
    decision: &Value,
    guardrails: &Value,
) {
    let Some(path) = x402_stellar_audit_path() else {
        return;
    };

    if let Some(parent) = Path::new(&path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Err(err) = fs::create_dir_all(parent) {
            logs.push(format!("x402_audit: mkdir_failed {err}"));
            return;
        }
    }

    let row = json!({
        "schema_version": 1,
        "service": "stellar.intent_plan",
        "endpoint": "/api/x402/stellar/intent-plan",
        "event": event,
        "timestamp": now_unix_secs(),
        "http_status": http_status.as_u16(),
        "audit_id": audit_id,
        "payment": payment,
        "decision": decision,
        "guardrails": guardrails
    });

    append_x402_audit_row(logs, &path, &row);
}

pub fn write_x402_settlement_audit_event(
    logs: &mut Vec<String>,
    event: &'static str,
    http_status: StatusCode,
    audit_id: &str,
    challenge_id: &str,
    settlement: &X402SettlementRecord,
) {
    let Some(path) = x402_stellar_audit_path() else {
        return;
    };

    if let Some(parent) = Path::new(&path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Err(err) = fs::create_dir_all(parent) {
            logs.push(format!("x402_audit: mkdir_failed {err}"));
            return;
        }
    }

    let row = x402_settlement_audit_row(event, http_status, audit_id, challenge_id, settlement);

    append_x402_audit_row(logs, &path, &row);
}

fn x402_settlement_audit_row(
    event: &'static str,
    http_status: StatusCode,
    audit_id: &str,
    challenge_id: &str,
    settlement: &X402SettlementRecord,
) -> Value {
    json!({
        "schema_version": 1,
        "service": "stellar.intent_plan",
        "endpoint": "/api/x402/stellar/intent-plan",
        "event": event,
        "timestamp": now_unix_secs(),
        "http_status": http_status.as_u16(),
        "audit_id": audit_id,
        "challenge_id": challenge_id,
        "request_digest": settlement.request_digest,
        "payment_state": settlement.state.as_str(),
        "verified_at": settlement.verified_at,
        "settlement_started_at": settlement.settlement_started_at,
        "settlement_completed_at": settlement.settlement_completed_at,
        "transaction_hash": settlement.transaction_hash,
        "underlying_action_submit_allowed": false
    })
}

fn append_x402_audit_row(logs: &mut Vec<String>, path: &str, row: &Value) {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{row}") {
                logs.push(format!("x402_audit: write_failed {err}"));
            } else {
                logs.push("x402_audit: wrote safe event".to_string());
            }
        }
        Err(err) => logs.push(format!("x402_audit: open_failed {err}")),
    }
}

fn x402_stellar_audit_path() -> Option<String> {
    env::var("NC_X402_STELLAR_AUDIT_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x402_store::X402SettlementState;

    #[test]
    fn settlement_audit_row_contains_only_bounded_public_evidence() {
        let record = X402SettlementRecord {
            request_digest: "a".repeat(64),
            state: X402SettlementState::SettlementOutcomeUnknown,
            verified_at: 10,
            settlement_started_at: Some(11),
            settlement_completed_at: Some(12),
            transaction_hash: None,
        };

        let row = x402_settlement_audit_row(
            "settlement_outcome_unknown",
            StatusCode::SERVICE_UNAVAILABLE,
            "audit-1",
            "x402s0001",
            &record,
        );
        let raw = row.to_string();

        assert_eq!(row["payment_state"], "settlement_outcome_unknown");
        assert_eq!(row["underlying_action_submit_allowed"], false);
        for forbidden in [
            "payment_payload",
            "payment_requirements",
            "payment_signature",
            "authorization",
            "secret",
        ] {
            assert!(!raw.contains(forbidden));
        }
    }
}
