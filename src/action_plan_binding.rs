use sha2::{Digest, Sha256};

use crate::actions::ActionPlan;

pub const ACTION_PLAN_HASH_DOMAIN: &[u8] = b"neurochain:mcp-v0:action-plan-json:v1\0";

pub fn canonical_action_plan_hash(plan: &ActionPlan) -> Result<String, String> {
    let encoded = serde_json::to_vec(plan)
        .map_err(|err| format!("failed to serialize canonical ActionPlan: {err}"))?;
    let mut hasher = Sha256::new();
    hasher.update(ACTION_PLAN_HASH_DOMAIN);
    hasher.update(encoded);
    Ok(hex::encode(hasher.finalize()))
}
