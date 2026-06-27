use serde::{Deserialize, Serialize};

pub type Digest32 = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofInput {
    pub evaluator_image_id: Digest32,
    pub action_plan: OwnedActionPlan,
    pub private_policy: OwnedPrivatePolicy,
    pub audit_nonce: Digest32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedActionPlan {
    pub schema_version: u32,
    pub intent_label: String,
    pub action_kind: String,
    pub contract_id: String,
    pub function: String,
    pub args: Vec<OwnedTypedArg>,
    pub intent_confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedTypedArg {
    pub name: String,
    pub value: OwnedTypedValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedTypedValue {
    Address(String),
    Bytes(Vec<u8>),
    Symbol(String),
    U64(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedPrivatePolicy {
    pub schema_version: u32,
    pub policy_version: u32,
    pub commitment_salt: Digest32,
    pub allowed_contracts: Vec<String>,
    pub allowed_contract_functions: Vec<String>,
    pub allowed_assets: Vec<String>,
    pub allowed_recipients: Vec<String>,
    pub max_amount_minor: u64,
    pub approval_threshold_minor: u64,
    pub min_intent_confidence_bps: u16,
}
