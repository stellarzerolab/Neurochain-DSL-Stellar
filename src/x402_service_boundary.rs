use serde::{Deserialize, Serialize};

use crate::{action_plan_binding::canonical_action_plan_hash, actions::ActionPlan};

pub const X402_SERVICE_BOUNDARY_SCHEMA_VERSION: u32 = 1;
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_RESOURCE_ID_BYTES: usize = 256;
const MAX_INTENT_TEXT_BYTES: usize = 4096;
const MAX_ACTION_PLAN_JSON_BYTES: usize = 65_536;
const MAX_ACTIONS_PER_PLAN: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402BoundaryMessageType {
    EvaluationRequest,
    EvaluationResponse,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum X402BoundaryNetwork {
    #[serde(rename = "stellar:testnet")]
    Testnet,
    #[serde(rename = "stellar:pubnet")]
    Pubnet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402BoundaryOperation {
    PlanAndEvaluate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X402BoundaryDecision {
    Approved,
    RequiresApproval,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct X402ServiceEvaluationRequest {
    pub schema_version: u32,
    pub message_type: X402BoundaryMessageType,
    pub request_id: String,
    pub resource_id: String,
    pub operation: X402BoundaryOperation,
    pub network: X402BoundaryNetwork,
    pub intent_text: String,
}

impl X402ServiceEvaluationRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_schema_and_message_type(
            self.schema_version,
            self.message_type,
            X402BoundaryMessageType::EvaluationRequest,
        )?;
        validate_bounded_text("request_id", &self.request_id, MAX_REQUEST_ID_BYTES)?;
        validate_bounded_text("resource_id", &self.resource_id, MAX_RESOURCE_ID_BYTES)?;
        validate_bounded_text("intent_text", &self.intent_text, MAX_INTENT_TEXT_BYTES)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct X402AuthorityGrants {
    pub payment_verification: bool,
    pub payment_settlement: bool,
    pub guardrail_override: bool,
    pub wallet_signing: bool,
    pub stellar_submission: bool,
}

impl X402AuthorityGrants {
    pub const fn none() -> Self {
        Self {
            payment_verification: false,
            payment_settlement: false,
            guardrail_override: false,
            wallet_signing: false,
            stellar_submission: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.payment_verification
            || self.payment_settlement
            || self.guardrail_override
            || self.wallet_signing
            || self.stellar_submission
        {
            return Err(
                "x402 service boundary responses must not grant payment, guardrail override, signing, or submission authority"
                    .to_string(),
            );
        }
        Ok(())
    }
}

impl Default for X402AuthorityGrants {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X402ServiceEvaluationResponse {
    pub schema_version: u32,
    pub message_type: X402BoundaryMessageType,
    pub request_id: String,
    pub decision: X402BoundaryDecision,
    pub exit_code: Option<i32>,
    pub reason_code: String,
    pub action_plan: ActionPlan,
    pub action_plan_hash: String,
    pub authority_grants: X402AuthorityGrants,
    pub underlying_action_submit_allowed: bool,
}

impl X402ServiceEvaluationResponse {
    pub fn validate(&self) -> Result<(), String> {
        validate_schema_and_message_type(
            self.schema_version,
            self.message_type,
            X402BoundaryMessageType::EvaluationResponse,
        )?;
        validate_bounded_text("request_id", &self.request_id, MAX_REQUEST_ID_BYTES)?;
        validate_bounded_text("reason_code", &self.reason_code, 128)?;

        if self.action_plan.actions.is_empty() {
            return Err("action_plan must contain at least one typed action".to_string());
        }
        if self.action_plan.actions.len() > MAX_ACTIONS_PER_PLAN {
            return Err(format!(
                "action_plan exceeds the {MAX_ACTIONS_PER_PLAN}-action boundary"
            ));
        }
        let encoded_plan = serde_json::to_vec(&self.action_plan)
            .map_err(|err| format!("failed to serialize action_plan: {err}"))?;
        if encoded_plan.len() > MAX_ACTION_PLAN_JSON_BYTES {
            return Err(format!(
                "action_plan exceeds the {MAX_ACTION_PLAN_JSON_BYTES}-byte boundary"
            ));
        }

        let expected_hash = canonical_action_plan_hash(&self.action_plan)?;
        if self.action_plan_hash != expected_hash {
            return Err("action_plan_hash does not match the canonical ActionPlan".to_string());
        }

        match self.decision {
            X402BoundaryDecision::Approved => {
                if self.exit_code.is_some() || self.reason_code != "approved" {
                    return Err(
                        "approved requires exit_code=null and reason_code=approved".to_string()
                    );
                }
            }
            X402BoundaryDecision::RequiresApproval => {
                if self.exit_code.is_some() || self.reason_code != "approval_required" {
                    return Err(
                        "requires_approval requires exit_code=null and reason_code=approval_required"
                            .to_string(),
                    );
                }
            }
            X402BoundaryDecision::Blocked => {
                if !matches!(self.exit_code, Some(3..=5)) {
                    return Err("blocked requires guardrail exit_code 3, 4, or 5".to_string());
                }
            }
        }

        self.authority_grants.validate()?;
        if self.underlying_action_submit_allowed {
            return Err(
                "underlying_action_submit_allowed must remain false at the service boundary"
                    .to_string(),
            );
        }
        Ok(())
    }
}

fn validate_schema_and_message_type(
    schema_version: u32,
    actual: X402BoundaryMessageType,
    expected: X402BoundaryMessageType,
) -> Result<(), String> {
    if schema_version != X402_SERVICE_BOUNDARY_SCHEMA_VERSION {
        return Err(format!(
            "unsupported x402 service boundary schema version {schema_version}"
        ));
    }
    if actual != expected {
        return Err("x402 service boundary message_type does not match the envelope".to_string());
    }
    Ok(())
}

fn validate_bounded_text(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    if value.len() > max_bytes {
        return Err(format!("{name} exceeds the {max_bytes}-byte boundary"));
    }
    Ok(())
}
