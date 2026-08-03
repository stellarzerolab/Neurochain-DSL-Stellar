use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402StellarChallenge {
    pub created_at: u64,
    pub expires_at: u64,
    pub finalized: bool,
    pub finalized_at: Option<u64>,
    pub payment_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum X402SettlementState {
    VerifiedPendingSettlement,
    SettlementInProgress,
    Settled,
    SettlementRejected,
    SettlementOutcomeUnknown,
}

impl X402SettlementState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VerifiedPendingSettlement => "verified_pending_settlement",
            Self::SettlementInProgress => "settlement_in_progress",
            Self::Settled => "settled",
            Self::SettlementRejected => "settlement_rejected",
            Self::SettlementOutcomeUnknown => "settlement_outcome_unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402SettlementRecord {
    pub request_digest: String,
    pub state: X402SettlementState,
    pub verified_at: u64,
    pub settlement_started_at: Option<u64>,
    pub settlement_completed_at: Option<u64>,
    pub transaction_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402RecordVerificationOutcome {
    Recorded(X402SettlementRecord),
    AlreadyRecorded(X402SettlementRecord),
    BindingMismatch,
    SettlementBlocked(X402SettlementRecord),
    ReplayBlocked(X402StellarChallenge),
    Expired(X402StellarChallenge),
    UnknownChallenge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402SettlementInspection {
    NotVerified,
    Recorded(X402SettlementRecord),
    UnknownChallenge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402BeginSettlementOutcome {
    Started(X402SettlementRecord),
    AlreadyInProgress(X402SettlementRecord),
    AlreadySettled(X402SettlementRecord),
    Blocked(X402SettlementRecord),
    BindingMismatch,
    NotVerified,
    ReplayBlocked(X402StellarChallenge),
    Expired(X402StellarChallenge),
    UnknownChallenge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402SettlementCompletion {
    Settled { transaction_hash: String },
    Rejected,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402CompleteSettlementOutcome {
    Completed(X402SettlementRecord),
    AlreadyCompleted(X402SettlementRecord),
    StateConflict(X402SettlementRecord),
    BindingMismatch,
    NotVerified,
    UnknownChallenge,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct X402StellarState {
    next_id: u64,
    #[serde(default)]
    challenges: HashMap<String, X402StellarChallenge>,
    #[serde(default)]
    used_challenges: HashSet<String>,
    #[serde(default)]
    settlements: HashMap<String, X402SettlementRecord>,
}

#[derive(Debug, Clone)]
pub struct X402ChallengeRecord {
    pub challenge_id: String,
    pub challenge: X402StellarChallenge,
}

#[derive(Debug, Clone)]
pub enum X402FinalizeOutcome {
    Finalized(X402StellarChallenge),
    ReplayBlocked(X402StellarChallenge),
    Expired(X402StellarChallenge),
    UnknownChallenge,
}

#[derive(Debug, Clone)]
pub enum X402ChallengeInspection {
    Available(X402StellarChallenge),
    ReplayBlocked(X402StellarChallenge),
    Expired(X402StellarChallenge),
    UnknownChallenge,
}

pub trait X402ChallengeStore {
    fn store_kind(&self) -> &'static str;
    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String>;
    fn inspect_challenge(&self, challenge_id: &str) -> Result<X402ChallengeInspection, String>;
    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String>;
    fn inspect_settlement(&self, challenge_id: &str) -> Result<X402SettlementInspection, String>;
    fn record_verified_payment(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402RecordVerificationOutcome, String>;
    fn begin_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402BeginSettlementOutcome, String>;
    fn complete_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
        completion: X402SettlementCompletion,
    ) -> Result<X402CompleteSettlementOutcome, String>;
}

impl X402StellarState {
    fn create_challenge(&mut self) -> X402ChallengeRecord {
        self.next_id += 1;
        let challenge_id = format!("x402s{:04}", self.next_id);
        let created_at = now_unix_secs();
        let expires_at = created_at.saturating_add(x402_stellar_ttl_secs());
        let challenge = X402StellarChallenge {
            created_at,
            expires_at,
            finalized: false,
            finalized_at: None,
            payment_state: "payment_required".to_string(),
        };
        self.challenges
            .insert(challenge_id.clone(), challenge.clone());
        X402ChallengeRecord {
            challenge_id,
            challenge,
        }
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> X402FinalizeOutcome {
        let used = self.used_challenges.contains(challenge_id);
        let Some(challenge) = self.challenges.get_mut(challenge_id) else {
            return X402FinalizeOutcome::UnknownChallenge;
        };

        if used || challenge.finalized {
            challenge.payment_state = "replay_blocked".to_string();
            return X402FinalizeOutcome::ReplayBlocked(challenge.clone());
        }

        if now_unix_secs() >= challenge.expires_at {
            challenge.payment_state = "expired".to_string();
            return X402FinalizeOutcome::Expired(challenge.clone());
        }

        let finalized_at = now_unix_secs();
        challenge.finalized = true;
        challenge.finalized_at = Some(finalized_at);
        challenge.payment_state = "finalized".to_string();
        self.used_challenges.insert(challenge_id.to_string());
        X402FinalizeOutcome::Finalized(challenge.clone())
    }

    fn inspect_challenge(&self, challenge_id: &str) -> X402ChallengeInspection {
        let Some(challenge) = self.challenges.get(challenge_id) else {
            return X402ChallengeInspection::UnknownChallenge;
        };
        let mut challenge = challenge.clone();

        if self.used_challenges.contains(challenge_id) || challenge.finalized {
            challenge.payment_state = "replay_blocked".to_string();
            return X402ChallengeInspection::ReplayBlocked(challenge);
        }
        if now_unix_secs() >= challenge.expires_at {
            challenge.payment_state = "expired".to_string();
            return X402ChallengeInspection::Expired(challenge);
        }

        X402ChallengeInspection::Available(challenge)
    }

    fn inspect_settlement(&self, challenge_id: &str) -> X402SettlementInspection {
        if !self.challenges.contains_key(challenge_id) {
            return X402SettlementInspection::UnknownChallenge;
        }
        self.settlements
            .get(challenge_id)
            .cloned()
            .map(X402SettlementInspection::Recorded)
            .unwrap_or(X402SettlementInspection::NotVerified)
    }

    fn record_verified_payment(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> X402RecordVerificationOutcome {
        if !is_request_digest(request_digest) {
            return X402RecordVerificationOutcome::BindingMismatch;
        }
        let used = self.used_challenges.contains(challenge_id);
        let Some(challenge) = self.challenges.get(challenge_id).cloned() else {
            return X402RecordVerificationOutcome::UnknownChallenge;
        };
        if used || challenge.finalized {
            return X402RecordVerificationOutcome::ReplayBlocked(challenge);
        }
        if now_unix_secs() >= challenge.expires_at {
            return X402RecordVerificationOutcome::Expired(challenge);
        }
        if let Some(record) = self.settlements.get(challenge_id).cloned() {
            if record.request_digest != request_digest {
                return X402RecordVerificationOutcome::BindingMismatch;
            }
            return if record.state == X402SettlementState::VerifiedPendingSettlement {
                X402RecordVerificationOutcome::AlreadyRecorded(record)
            } else {
                X402RecordVerificationOutcome::SettlementBlocked(record)
            };
        }

        let record = X402SettlementRecord {
            request_digest: request_digest.to_string(),
            state: X402SettlementState::VerifiedPendingSettlement,
            verified_at: now_unix_secs(),
            settlement_started_at: None,
            settlement_completed_at: None,
            transaction_hash: None,
        };
        self.settlements
            .insert(challenge_id.to_string(), record.clone());
        X402RecordVerificationOutcome::Recorded(record)
    }

    fn begin_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> X402BeginSettlementOutcome {
        let used = self.used_challenges.contains(challenge_id);
        let Some(challenge) = self.challenges.get(challenge_id).cloned() else {
            return X402BeginSettlementOutcome::UnknownChallenge;
        };
        if used || challenge.finalized {
            if let Some(record) = self.settlements.get(challenge_id).cloned() {
                if record.request_digest == request_digest
                    && record.state == X402SettlementState::Settled
                {
                    return X402BeginSettlementOutcome::AlreadySettled(record);
                }
            }
            return X402BeginSettlementOutcome::ReplayBlocked(challenge);
        }
        if now_unix_secs() >= challenge.expires_at {
            return X402BeginSettlementOutcome::Expired(challenge);
        }
        let Some(record) = self.settlements.get_mut(challenge_id) else {
            return X402BeginSettlementOutcome::NotVerified;
        };
        if record.request_digest != request_digest {
            return X402BeginSettlementOutcome::BindingMismatch;
        }

        match record.state {
            X402SettlementState::VerifiedPendingSettlement => {
                record.state = X402SettlementState::SettlementInProgress;
                record.settlement_started_at = Some(now_unix_secs());
                X402BeginSettlementOutcome::Started(record.clone())
            }
            X402SettlementState::SettlementInProgress => {
                X402BeginSettlementOutcome::AlreadyInProgress(record.clone())
            }
            X402SettlementState::Settled => {
                X402BeginSettlementOutcome::AlreadySettled(record.clone())
            }
            X402SettlementState::SettlementRejected
            | X402SettlementState::SettlementOutcomeUnknown => {
                X402BeginSettlementOutcome::Blocked(record.clone())
            }
        }
    }

    fn complete_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
        completion: X402SettlementCompletion,
    ) -> X402CompleteSettlementOutcome {
        if !self.challenges.contains_key(challenge_id) {
            return X402CompleteSettlementOutcome::UnknownChallenge;
        }
        let Some(record) = self.settlements.get_mut(challenge_id) else {
            return X402CompleteSettlementOutcome::NotVerified;
        };
        if record.request_digest != request_digest {
            return X402CompleteSettlementOutcome::BindingMismatch;
        }
        if record.state == X402SettlementState::Settled {
            return X402CompleteSettlementOutcome::AlreadyCompleted(record.clone());
        }
        if record.state != X402SettlementState::SettlementInProgress {
            return X402CompleteSettlementOutcome::StateConflict(record.clone());
        }

        let completed_at = now_unix_secs();
        match completion {
            X402SettlementCompletion::Settled { transaction_hash } => {
                if !is_stellar_transaction_hash(&transaction_hash) {
                    return X402CompleteSettlementOutcome::StateConflict(record.clone());
                }
                record.state = X402SettlementState::Settled;
                record.transaction_hash = Some(transaction_hash);
                let challenge = self
                    .challenges
                    .get_mut(challenge_id)
                    .expect("challenge existence checked before settlement completion");
                challenge.finalized = true;
                challenge.finalized_at = Some(completed_at);
                challenge.payment_state = "settled".to_string();
                self.used_challenges.insert(challenge_id.to_string());
            }
            X402SettlementCompletion::Rejected => {
                record.state = X402SettlementState::SettlementRejected;
            }
            X402SettlementCompletion::OutcomeUnknown => {
                record.state = X402SettlementState::SettlementOutcomeUnknown;
            }
        }
        record.settlement_completed_at = Some(completed_at);
        X402CompleteSettlementOutcome::Completed(record.clone())
    }
}

#[derive(Debug, Default)]
struct InMemoryX402ChallengeStore {
    state: X402StellarState,
}

#[derive(Debug)]
struct UnavailableX402ChallengeStore {
    error: String,
}

impl X402ChallengeStore for InMemoryX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "in_memory"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        Ok(self.state.create_challenge())
    }

    fn inspect_challenge(&self, challenge_id: &str) -> Result<X402ChallengeInspection, String> {
        Ok(self.state.inspect_challenge(challenge_id))
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        Ok(self.state.begin_finalize(challenge_id))
    }

    fn inspect_settlement(&self, challenge_id: &str) -> Result<X402SettlementInspection, String> {
        Ok(self.state.inspect_settlement(challenge_id))
    }

    fn record_verified_payment(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402RecordVerificationOutcome, String> {
        Ok(self
            .state
            .record_verified_payment(challenge_id, request_digest))
    }

    fn begin_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402BeginSettlementOutcome, String> {
        Ok(self.state.begin_settlement(challenge_id, request_digest))
    }

    fn complete_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
        completion: X402SettlementCompletion,
    ) -> Result<X402CompleteSettlementOutcome, String> {
        Ok(self
            .state
            .complete_settlement(challenge_id, request_digest, completion))
    }
}

impl X402ChallengeStore for UnavailableX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "unavailable"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        Err(self.error.clone())
    }

    fn inspect_challenge(&self, _challenge_id: &str) -> Result<X402ChallengeInspection, String> {
        Err(self.error.clone())
    }

    fn begin_finalize(&mut self, _challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        Err(self.error.clone())
    }

    fn inspect_settlement(&self, _challenge_id: &str) -> Result<X402SettlementInspection, String> {
        Err(self.error.clone())
    }

    fn record_verified_payment(
        &mut self,
        _challenge_id: &str,
        _request_digest: &str,
    ) -> Result<X402RecordVerificationOutcome, String> {
        Err(self.error.clone())
    }

    fn begin_settlement(
        &mut self,
        _challenge_id: &str,
        _request_digest: &str,
    ) -> Result<X402BeginSettlementOutcome, String> {
        Err(self.error.clone())
    }

    fn complete_settlement(
        &mut self,
        _challenge_id: &str,
        _request_digest: &str,
        _completion: X402SettlementCompletion,
    ) -> Result<X402CompleteSettlementOutcome, String> {
        Err(self.error.clone())
    }
}

#[derive(Debug)]
struct FileX402ChallengeStore {
    path: PathBuf,
    state: X402StellarState,
}

impl FileX402ChallengeStore {
    fn load(path: PathBuf) -> Result<Self, String> {
        let backup_path = path.with_extension("json.bak");
        let (mut state, recovered_from_backup) = match fs::read_to_string(&path) {
            Ok(raw) if raw.trim().is_empty() => (X402StellarState::default(), false),
            Ok(raw) => (
                serde_json::from_str(&raw).map_err(|err| {
                    format!("x402 store parse failed at {}: {err}", path.display())
                })?,
                false,
            ),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && backup_path.exists() => {
                let raw = fs::read_to_string(&backup_path).map_err(|backup_error| {
                    format!(
                        "x402 store recovery read failed at {}: {backup_error}",
                        backup_path.display()
                    )
                })?;
                (
                    serde_json::from_str(&raw).map_err(|parse_error| {
                        format!(
                            "x402 store recovery parse failed at {}: {parse_error}",
                            backup_path.display()
                        )
                    })?,
                    true,
                )
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                (X402StellarState::default(), false)
            }
            Err(err) => {
                return Err(format!(
                    "x402 store read failed at {}: {err}",
                    path.display()
                ));
            }
        };

        if recovered_from_backup {
            fs::rename(&backup_path, &path).map_err(|err| {
                format!(
                    "x402 store recovery restore failed from {} to {}: {err}",
                    backup_path.display(),
                    path.display()
                )
            })?;
        }

        let recovered_at = now_unix_secs();
        let mut recovered_in_progress = false;
        for record in state.settlements.values_mut() {
            if record.state == X402SettlementState::SettlementInProgress {
                record.state = X402SettlementState::SettlementOutcomeUnknown;
                record.settlement_completed_at = Some(recovered_at);
                recovered_in_progress = true;
            }
        }
        validate_loaded_state(&state, &path)?;

        let store = Self { path, state };
        if recovered_in_progress {
            store.persist()?;
        }
        Ok(store)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "x402 store mkdir failed at {}: {err}",
                    parent.to_string_lossy()
                )
            })?;
        }

        let raw = serde_json::to_vec_pretty(&self.state)
            .map_err(|err| format!("x402 store serialize failed: {err}"))?;
        let tmp_path = self.path.with_extension("json.tmp");
        let backup_path = self.path.with_extension("json.bak");
        let mut tmp = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp_path)
            .map_err(|err| format!("x402 store write failed at {}: {err}", tmp_path.display()))?;
        use std::io::Write as _;
        tmp.write_all(&raw)
            .and_then(|_| tmp.sync_all())
            .map_err(|err| format!("x402 store sync failed at {}: {err}", tmp_path.display()))?;
        drop(tmp);

        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|err| {
                format!(
                    "x402 store stale backup removal failed at {}: {err}",
                    backup_path.display()
                )
            })?;
        }
        if self.path.exists() {
            fs::rename(&self.path, &backup_path).map_err(|err| {
                format!(
                    "x402 store backup failed from {} to {}: {err}",
                    self.path.display(),
                    backup_path.display()
                )
            })?;
        }
        if let Err(err) = fs::rename(&tmp_path, &self.path) {
            if backup_path.exists() && !self.path.exists() {
                let _ = fs::rename(&backup_path, &self.path);
            }
            return Err(format!(
                "x402 store replace failed from {} to {}: {err}",
                tmp_path.display(),
                self.path.display()
            ));
        }
        if backup_path.exists() {
            let _ = fs::remove_file(backup_path);
        }
        Ok(())
    }
}

impl X402ChallengeStore for FileX402ChallengeStore {
    fn store_kind(&self) -> &'static str {
        "file"
    }

    fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
        let previous = self.state.clone();
        let record = self.state.create_challenge();
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(error);
        }
        Ok(record)
    }

    fn inspect_challenge(&self, challenge_id: &str) -> Result<X402ChallengeInspection, String> {
        Ok(self.state.inspect_challenge(challenge_id))
    }

    fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
        let previous = self.state.clone();
        let outcome = self.state.begin_finalize(challenge_id);
        if !matches!(outcome, X402FinalizeOutcome::UnknownChallenge) {
            if let Err(error) = self.persist() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(outcome)
    }

    fn inspect_settlement(&self, challenge_id: &str) -> Result<X402SettlementInspection, String> {
        Ok(self.state.inspect_settlement(challenge_id))
    }

    fn record_verified_payment(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402RecordVerificationOutcome, String> {
        let previous = self.state.clone();
        let outcome = self
            .state
            .record_verified_payment(challenge_id, request_digest);
        if matches!(outcome, X402RecordVerificationOutcome::Recorded(_)) {
            if let Err(error) = self.persist() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(outcome)
    }

    fn begin_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
    ) -> Result<X402BeginSettlementOutcome, String> {
        let previous = self.state.clone();
        let outcome = self.state.begin_settlement(challenge_id, request_digest);
        if matches!(outcome, X402BeginSettlementOutcome::Started(_)) {
            if let Err(error) = self.persist() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(outcome)
    }

    fn complete_settlement(
        &mut self,
        challenge_id: &str,
        request_digest: &str,
        completion: X402SettlementCompletion,
    ) -> Result<X402CompleteSettlementOutcome, String> {
        let previous = self.state.clone();
        let outcome = self
            .state
            .complete_settlement(challenge_id, request_digest, completion);
        if matches!(outcome, X402CompleteSettlementOutcome::Completed(_)) {
            if let Err(error) = self.persist() {
                self.state = previous;
                return Err(error);
            }
        }
        Ok(outcome)
    }
}

pub fn build_x402_challenge_store() -> Box<dyn X402ChallengeStore + Send> {
    let Some(path) = x402_stellar_store_path() else {
        return Box::<InMemoryX402ChallengeStore>::default();
    };

    match FileX402ChallengeStore::load(path.clone()) {
        Ok(store) => Box::new(store),
        Err(err) => {
            eprintln!("ERROR: {err}; x402 challenge store unavailable");
            Box::new(UnavailableX402ChallengeStore { error: err })
        }
    }
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn x402_stellar_ttl_secs() -> u64 {
    env::var("NC_X402_STELLAR_TTL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(300)
}

fn x402_stellar_store_path() -> Option<PathBuf> {
    env::var("NC_X402_STELLAR_STORE_PATH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn is_request_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_stellar_transaction_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_loaded_state(state: &X402StellarState, path: &Path) -> Result<(), String> {
    for challenge_id in &state.used_challenges {
        let valid = state
            .challenges
            .get(challenge_id)
            .is_some_and(|challenge| challenge.finalized);
        if !valid {
            return Err(format!(
                "x402 store invariant failed at {}: used challenge is missing or unfinalized",
                path.display()
            ));
        }
    }

    for (challenge_id, settlement) in &state.settlements {
        let Some(challenge) = state.challenges.get(challenge_id) else {
            return Err(format!(
                "x402 store invariant failed at {}: settlement has no challenge",
                path.display()
            ));
        };
        if !is_request_digest(&settlement.request_digest) {
            return Err(format!(
                "x402 store invariant failed at {}: settlement request digest is invalid",
                path.display()
            ));
        }
        if settlement
            .transaction_hash
            .as_deref()
            .is_some_and(|hash| !is_stellar_transaction_hash(hash))
        {
            return Err(format!(
                "x402 store invariant failed at {}: settlement transaction hash is invalid",
                path.display()
            ));
        }

        if settlement.state == X402SettlementState::Settled {
            if settlement.transaction_hash.is_none()
                || !challenge.finalized
                || !state.used_challenges.contains(challenge_id)
            {
                return Err(format!(
                    "x402 store invariant failed at {}: settled payment is not atomically finalized",
                    path.display()
                ));
            }
        } else if settlement.transaction_hash.is_some() || challenge.finalized {
            return Err(format!(
                "x402 store invariant failed at {}: unsettled payment contains finalized evidence",
                path.display()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TX_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn state_with_challenge() -> X402StellarState {
        let mut state = X402StellarState::default();
        state.challenges.insert(
            "x402s0001".to_string(),
            X402StellarChallenge {
                created_at: 1,
                expires_at: u64::MAX,
                finalized: false,
                finalized_at: None,
                payment_state: "payment_required".to_string(),
            },
        );
        state
    }

    fn temporary_store_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "neurochain-x402-{name}-{}-{}.json",
            std::process::id(),
            now_unix_secs()
        ))
    }

    #[test]
    fn verified_payment_binding_is_idempotent_and_mismatch_fails_closed() {
        let mut state = state_with_challenge();

        let recorded = state.record_verified_payment("x402s0001", DIGEST_A);
        assert!(matches!(
            recorded,
            X402RecordVerificationOutcome::Recorded(_)
        ));
        assert!(matches!(
            state.record_verified_payment("x402s0001", DIGEST_A),
            X402RecordVerificationOutcome::AlreadyRecorded(_)
        ));
        assert_eq!(
            state.record_verified_payment("x402s0001", DIGEST_B),
            X402RecordVerificationOutcome::BindingMismatch
        );
    }

    #[test]
    fn settlement_transition_is_single_start_and_finalizes_only_on_success() {
        let mut state = state_with_challenge();
        state.record_verified_payment("x402s0001", DIGEST_A);

        assert!(matches!(
            state.begin_settlement("x402s0001", DIGEST_A),
            X402BeginSettlementOutcome::Started(_)
        ));
        assert!(matches!(
            state.begin_settlement("x402s0001", DIGEST_A),
            X402BeginSettlementOutcome::AlreadyInProgress(_)
        ));
        assert_eq!(
            state.begin_settlement("x402s0001", DIGEST_B),
            X402BeginSettlementOutcome::BindingMismatch
        );

        let completed = state.complete_settlement(
            "x402s0001",
            DIGEST_A,
            X402SettlementCompletion::Settled {
                transaction_hash: TX_HASH.to_string(),
            },
        );
        assert!(matches!(
            completed,
            X402CompleteSettlementOutcome::Completed(X402SettlementRecord {
                state: X402SettlementState::Settled,
                ..
            })
        ));
        assert!(state.used_challenges.contains("x402s0001"));
        assert!(state.challenges["x402s0001"].finalized);
        assert!(matches!(
            state.begin_settlement("x402s0001", DIGEST_A),
            X402BeginSettlementOutcome::AlreadySettled(_)
        ));
    }

    #[test]
    fn uncertain_settlement_is_persisted_and_never_retried_automatically() {
        let path = temporary_store_path("outcome-unknown");
        let mut store = FileX402ChallengeStore {
            path: path.clone(),
            state: state_with_challenge(),
        };
        store.persist().unwrap();
        store
            .record_verified_payment("x402s0001", DIGEST_A)
            .unwrap();
        assert!(matches!(
            store.begin_settlement("x402s0001", DIGEST_A).unwrap(),
            X402BeginSettlementOutcome::Started(_)
        ));

        drop(store);
        let mut recovered = FileX402ChallengeStore::load(path.clone()).unwrap();
        let record = match recovered.inspect_settlement("x402s0001").unwrap() {
            X402SettlementInspection::Recorded(record) => record,
            other => panic!("expected recovered settlement record, got {other:?}"),
        };
        assert_eq!(record.state, X402SettlementState::SettlementOutcomeUnknown);
        assert!(matches!(
            recovered.begin_settlement("x402s0001", DIGEST_A).unwrap(),
            X402BeginSettlementOutcome::Blocked(X402SettlementRecord {
                state: X402SettlementState::SettlementOutcomeUnknown,
                ..
            })
        ));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn explicit_rejection_is_terminal_without_finalizing_the_challenge() {
        let mut state = state_with_challenge();
        state.record_verified_payment("x402s0001", DIGEST_A);
        state.begin_settlement("x402s0001", DIGEST_A);

        assert!(matches!(
            state.complete_settlement("x402s0001", DIGEST_A, X402SettlementCompletion::Rejected,),
            X402CompleteSettlementOutcome::Completed(X402SettlementRecord {
                state: X402SettlementState::SettlementRejected,
                ..
            })
        ));
        assert!(!state.challenges["x402s0001"].finalized);
        assert!(!state.used_challenges.contains("x402s0001"));
        assert!(matches!(
            state.begin_settlement("x402s0001", DIGEST_A),
            X402BeginSettlementOutcome::Blocked(_)
        ));
    }
}
