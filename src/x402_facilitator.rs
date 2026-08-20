use std::{collections::BTreeMap, env, fmt, io::Read, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::x402_store::{
    X402BeginSettlementOutcome, X402ChallengeInspection, X402ChallengeRecord, X402ChallengeStore,
    X402CompleteSettlementOutcome, X402FinalizeOutcome, X402RecordVerificationOutcome,
    X402SettlementCompletion, X402SettlementInspection, X402SettlementState, X402StellarChallenge,
};

pub const X402_FACILITATOR_PROTOCOL_VERSION: u8 = 2;
pub const X402_FACILITATOR_API_KEY_ENV: &str = "NC_X402_FACILITATOR_API_KEY";
const MIN_FACILITATOR_TIMEOUT_MS: u64 = 100;
const MAX_FACILITATOR_TIMEOUT_MS: u64 = 30_000;
const MAX_FACILITATOR_RESPONSE_BYTES: u64 = 256 * 1024;
const MAX_X402_HEADER_BYTES: usize = 256 * 1024;
const MAX_X402_HEADER_ENCODED_BYTES: usize = 384 * 1024;
const X402_FACILITATOR_ENDPOINT_ENV: &str = "NC_X402_FACILITATOR_ENDPOINT";
const X402_FACILITATOR_TIMEOUT_MS_ENV: &str = "NC_X402_FACILITATOR_TIMEOUT_MS";
const X402_FACILITATOR_RESOURCE_URL_ENV: &str = "NC_X402_FACILITATOR_RESOURCE_URL";
const X402_STELLAR_AMOUNT_ENV: &str = "NC_X402_STELLAR_AMOUNT";
const X402_STELLAR_ASSET_ENV: &str = "NC_X402_STELLAR_ASSET";
const X402_STELLAR_NETWORK_ENV: &str = "NC_X402_STELLAR_NETWORK";
const X402_STELLAR_RECEIVER_ENV: &str = "NC_X402_STELLAR_RECEIVER";
const X402_STELLAR_MAX_TIMEOUT_SECONDS_ENV: &str = "NC_X402_STELLAR_MAX_TIMEOUT_SECONDS";
const X402_STELLAR_STORE_PATH_ENV: &str = "NC_X402_STELLAR_STORE_PATH";
const X402_STELLAR_AUDIT_PATH_ENV: &str = "NC_X402_STELLAR_AUDIT_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402FacilitatorConfig {
    pub endpoint: String,
    pub network: String,
    pub asset_contract: String,
    pub receiver: String,
    pub timeout_ms: u64,
}

impl X402FacilitatorConfig {
    pub fn validate(
        endpoint: &str,
        network: &str,
        asset_contract: &str,
        receiver: &str,
        timeout_ms: u64,
    ) -> Result<Self, X402FacilitatorTransportError> {
        let endpoint = endpoint.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(endpoint).map_err(|err| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "invalid facilitator endpoint: {err}"
            ))
        })?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator endpoint must be a credential-free HTTPS base URL without query or fragment"
                    .to_string(),
            ));
        }

        let network = network.trim();
        if !matches!(network, "stellar:testnet" | "stellar:pubnet") {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator network must be stellar:testnet or stellar:pubnet".to_string(),
            ));
        }
        if !is_stellar_strkey(asset_contract.trim(), &['C']) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "asset_contract must be a Stellar contract StrKey".to_string(),
            ));
        }
        if !is_stellar_strkey(receiver.trim(), &['G', 'C', 'M']) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "receiver must be a Stellar account, contract, or muxed-account StrKey".to_string(),
            ));
        }
        if !(MIN_FACILITATOR_TIMEOUT_MS..=MAX_FACILITATOR_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                format!(
                    "facilitator timeout must be between {MIN_FACILITATOR_TIMEOUT_MS} and {MAX_FACILITATOR_TIMEOUT_MS} milliseconds"
                ),
            ));
        }

        Ok(Self {
            endpoint: endpoint.to_string(),
            network: network.to_string(),
            asset_contract: asset_contract.trim().to_string(),
            receiver: receiver.trim().to_string(),
            timeout_ms,
        })
    }

    pub fn verify_url(&self) -> String {
        format!("{}/verify", self.endpoint)
    }

    pub fn settle_url(&self) -> String {
        format!("{}/settle", self.endpoint)
    }

    pub fn supported_url(&self) -> String {
        format!("{}/supported", self.endpoint)
    }

    pub fn validate_supported(
        &self,
        response: &X402FacilitatorSupportedResponse,
    ) -> Result<(), X402FacilitatorTransportError> {
        let supported = response.kinds.iter().any(|kind| {
            kind.x402_version == X402_FACILITATOR_PROTOCOL_VERSION
                && matches!(kind.scheme.as_str(), "exact" | "exact-v2")
                && kind.network == self.network
        });
        if supported {
            Ok(())
        } else {
            Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator does not advertise x402 v2 exact support for the configured Stellar network"
                    .to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct X402PaymentRequiredPresentation {
    pub encoded_header: Option<String>,
    pub payment_required: Option<Value>,
    pub mock_signature: Option<String>,
}

impl X402PaymentRequiredPresentation {
    fn mock(challenge_id: &str) -> Self {
        Self {
            encoded_header: None,
            payment_required: None,
            mock_signature: Some(format!("paid:{challenge_id}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct X402FacilitatorRuntimeConfig {
    transport: X402FacilitatorConfig,
    amount: String,
    max_timeout_seconds: u64,
    resource_url: String,
}

impl X402FacilitatorRuntimeConfig {
    fn from_env() -> Result<Self, X402FacilitatorTransportError> {
        let endpoint = required_runtime_env(X402_FACILITATOR_ENDPOINT_ENV)?;
        let network = required_runtime_env(X402_STELLAR_NETWORK_ENV)?;
        let asset_contract = required_runtime_env(X402_STELLAR_ASSET_ENV)?;
        let receiver = required_runtime_env(X402_STELLAR_RECEIVER_ENV)?;
        let amount = required_runtime_env(X402_STELLAR_AMOUNT_ENV)?;
        let resource_url = required_runtime_env(X402_FACILITATOR_RESOURCE_URL_ENV)?;
        required_runtime_env(X402_STELLAR_STORE_PATH_ENV)?;
        required_runtime_env(X402_STELLAR_AUDIT_PATH_ENV)?;

        let timeout_ms = optional_runtime_u64(X402_FACILITATOR_TIMEOUT_MS_ENV, 5_000)?;
        let max_timeout_seconds = optional_runtime_u64(X402_STELLAR_MAX_TIMEOUT_SECONDS_ENV, 60)?;
        if max_timeout_seconds == 0 {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator max timeout must be positive".to_string(),
            ));
        }
        if amount.is_empty()
            || !amount.bytes().all(|byte| byte.is_ascii_digit())
            || amount.bytes().all(|byte| byte == b'0')
        {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator amount must be a positive base-unit integer string".to_string(),
            ));
        }
        validate_resource_url(&resource_url)?;

        Ok(Self {
            transport: X402FacilitatorConfig::validate(
                &endpoint,
                &network,
                &asset_contract,
                &receiver,
                timeout_ms,
            )?,
            amount,
            max_timeout_seconds,
            resource_url,
        })
    }

    fn payment_requirements(&self, challenge_id: &str) -> Value {
        serde_json::json!({
            "scheme": "exact",
            "network": self.transport.network,
            "asset": self.transport.asset_contract,
            "amount": self.amount,
            "payTo": self.transport.receiver,
            "maxTimeoutSeconds": self.max_timeout_seconds,
            "extra": {
                "areFeesSponsored": true,
                "neurochainChallengeId": challenge_id,
            }
        })
    }

    fn resource(&self) -> Value {
        serde_json::json!({
            "url": self.resource_url,
            "description": "NeuroChain typed Stellar ActionPlan evaluation",
            "mimeType": "application/json",
        })
    }

    fn payment_required(&self, challenge_id: &str) -> Value {
        serde_json::json!({
            "x402Version": X402_FACILITATOR_PROTOCOL_VERSION,
            "error": "PAYMENT-SIGNATURE header is required",
            "resource": self.resource(),
            "accepts": [self.payment_requirements(challenge_id)],
            "extensions": {
                "neurochain": {
                    "challengeId": challenge_id,
                    "settlementRequired": true,
                    "underlyingActionSubmitAllowed": false,
                }
            }
        })
    }

    fn payment_required_presentation(
        &self,
        challenge_id: &str,
    ) -> Result<X402PaymentRequiredPresentation, X402FacilitatorTransportError> {
        let payment_required = self.payment_required(challenge_id);
        let encoded_header = encode_x402_header(&payment_required)?;
        Ok(X402PaymentRequiredPresentation {
            encoded_header: Some(encoded_header),
            payment_required: Some(payment_required),
            mock_signature: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct X402FacilitatorRequest {
    pub x402_version: u8,
    pub payment_payload: Value,
    pub payment_requirements: Value,
    pub idempotency_key: String,
    pub network: String,
}

pub fn facilitator_request_digest(
    request: &X402FacilitatorRequest,
) -> Result<String, X402FacilitatorTransportError> {
    let value = serde_json::json!({
        "x402Version": request.x402_version,
        "paymentPayload": request.payment_payload,
        "paymentRequirements": request.payment_requirements,
        "idempotencyKey": request.idempotency_key,
        "network": request.network,
    });
    let mut hasher = Sha256::new();
    update_json_digest(&mut hasher, &value)?;
    Ok(hex::encode(hasher.finalize()))
}

fn update_json_digest(
    hasher: &mut Sha256,
    value: &Value,
) -> Result<(), X402FacilitatorTransportError> {
    match value {
        Value::Null => hasher.update([0]),
        Value::Bool(boolean) => hasher.update([1, u8::from(*boolean)]),
        Value::Number(number) => {
            hasher.update([2]);
            update_digest_bytes(hasher, number.to_string().as_bytes());
        }
        Value::String(string) => {
            hasher.update([3]);
            update_digest_bytes(hasher, string.as_bytes());
        }
        Value::Array(values) => {
            hasher.update([4]);
            update_digest_length(hasher, values.len())?;
            for value in values {
                update_json_digest(hasher, value)?;
            }
        }
        Value::Object(object) => {
            hasher.update([5]);
            update_digest_length(hasher, object.len())?;
            let mut fields: Vec<_> = object.iter().collect();
            fields.sort_unstable_by_key(|(field, _)| *field);
            for (field, value) in fields {
                update_digest_bytes(hasher, field.as_bytes());
                update_json_digest(hasher, value)?;
            }
        }
    }
    Ok(())
}

fn update_digest_length(
    hasher: &mut Sha256,
    length: usize,
) -> Result<(), X402FacilitatorTransportError> {
    let length = u64::try_from(length).map_err(|_| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator request is too large to bind".to_string(),
        )
    })?;
    hasher.update(length.to_be_bytes());
    Ok(())
}

fn update_digest_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorVerifyResponse {
    pub is_valid: bool,
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorSettleResponse {
    pub success: bool,
    pub transaction_hash: Option<String>,
    pub error_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X402FacilitatorSupportedKind {
    pub x402_version: u8,
    pub scheme: String,
    pub network: String,
    #[serde(default)]
    pub extra: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorSupportedResponse {
    pub kinds: Vec<X402FacilitatorSupportedKind>,
    #[serde(default)]
    pub signers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub extensions: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402FacilitatorTransportError {
    InvalidConfiguration(String),
    Unavailable(String),
    Timeout,
    InvalidResponse(String),
}

pub trait X402FacilitatorCredentialProvider {
    fn provider_kind(&self) -> &'static str;
    fn authorization_header(&self) -> Result<HeaderValue, X402FacilitatorTransportError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct EnvX402FacilitatorCredentialProvider {
    variable: String,
}

impl EnvX402FacilitatorCredentialProvider {
    pub fn new(variable: impl Into<String>) -> Result<Self, X402FacilitatorTransportError> {
        let variable = variable.into();
        let variable = variable.trim();
        if variable.is_empty() || variable.contains('=') || variable.contains('\0') {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator credential environment variable name is invalid".to_string(),
            ));
        }
        Ok(Self {
            variable: variable.to_string(),
        })
    }

    pub fn variable(&self) -> &str {
        &self.variable
    }
}

impl Default for EnvX402FacilitatorCredentialProvider {
    fn default() -> Self {
        Self {
            variable: X402_FACILITATOR_API_KEY_ENV.to_string(),
        }
    }
}

impl fmt::Debug for EnvX402FacilitatorCredentialProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvX402FacilitatorCredentialProvider")
            .field("variable", &self.variable)
            .field("credential", &"<runtime-only>")
            .finish()
    }
}

impl X402FacilitatorCredentialProvider for EnvX402FacilitatorCredentialProvider {
    fn provider_kind(&self) -> &'static str {
        "environment"
    }

    fn authorization_header(&self) -> Result<HeaderValue, X402FacilitatorTransportError> {
        let token = env::var(&self.variable).map_err(|_| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "facilitator credential is unavailable from runtime environment variable {}",
                self.variable
            ))
        })?;
        let token = token.trim();
        if token.is_empty() {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator credential is empty".to_string(),
            ));
        }

        let mut header = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator credential cannot be encoded as an authorization header".to_string(),
            )
        })?;
        header.set_sensitive(true);
        Ok(header)
    }
}

pub struct ReqwestX402FacilitatorTransport<P> {
    config: X402FacilitatorConfig,
    credential_provider: P,
}

impl<P> ReqwestX402FacilitatorTransport<P>
where
    P: X402FacilitatorCredentialProvider,
{
    pub fn new(
        config: X402FacilitatorConfig,
        credential_provider: P,
    ) -> Result<Self, X402FacilitatorTransportError> {
        let config = X402FacilitatorConfig::validate(
            &config.endpoint,
            &config.network,
            &config.asset_contract,
            &config.receiver,
            config.timeout_ms,
        )?;
        Ok(Self {
            config,
            credential_provider,
        })
    }

    fn http_client(&self) -> Result<reqwest::blocking::Client, X402FacilitatorTransportError> {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(self.config.timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                X402FacilitatorTransportError::InvalidConfiguration(
                    "facilitator HTTP client could not be constructed".to_string(),
                )
            })
    }

    fn build_supported_request(
        &self,
    ) -> Result<reqwest::blocking::Request, X402FacilitatorTransportError> {
        self.http_client()?
            .get(self.config.supported_url())
            .header(ACCEPT, "application/json")
            .header(
                AUTHORIZATION,
                self.credential_provider.authorization_header()?,
            )
            .build()
            .map_err(|_| {
                X402FacilitatorTransportError::InvalidConfiguration(
                    "facilitator supported request could not be constructed".to_string(),
                )
            })
    }

    fn execute_supported(
        &self,
        request: reqwest::blocking::Request,
    ) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError> {
        let response = self
            .http_client()?
            .execute(request)
            .map_err(map_reqwest_error)?;
        ensure_supported_status(response.status())?;

        if response
            .content_length()
            .is_some_and(|length| length > MAX_FACILITATOR_RESPONSE_BYTES)
        {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "facilitator response exceeds the size limit".to_string(),
            ));
        }

        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let mut body = Vec::new();
        response
            .take(MAX_FACILITATOR_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    X402FacilitatorTransportError::Timeout
                } else {
                    X402FacilitatorTransportError::Unavailable(
                        "facilitator response body could not be read".to_string(),
                    )
                }
            })?;
        parse_supported_response(content_type.as_ref(), &body)
    }

    fn build_verify_request(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<reqwest::blocking::Request, X402FacilitatorTransportError> {
        validate_verify_wire_request(&self.config, request)?;
        let body = serde_json::json!({
            "x402Version": request.x402_version,
            "paymentPayload": request.payment_payload,
            "paymentRequirements": request.payment_requirements,
        });

        self.http_client()?
            .post(self.config.verify_url())
            .header(ACCEPT, "application/json")
            .header(
                AUTHORIZATION,
                self.credential_provider.authorization_header()?,
            )
            .json(&body)
            .build()
            .map_err(|_| {
                X402FacilitatorTransportError::InvalidConfiguration(
                    "facilitator verify request could not be constructed".to_string(),
                )
            })
    }

    fn execute_verify(
        &self,
        request: reqwest::blocking::Request,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
        let response = self
            .http_client()?
            .execute(request)
            .map_err(map_reqwest_error)?;
        let status = response.status();
        ensure_verify_status(status)?;

        if response
            .content_length()
            .is_some_and(|length| length > MAX_FACILITATOR_RESPONSE_BYTES)
        {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "facilitator response exceeds the size limit".to_string(),
            ));
        }

        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let mut body = Vec::new();
        response
            .take(MAX_FACILITATOR_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    X402FacilitatorTransportError::Timeout
                } else {
                    X402FacilitatorTransportError::Unavailable(
                        "facilitator response body could not be read".to_string(),
                    )
                }
            })?;
        parse_verify_response(status, content_type.as_ref(), &body)
    }

    fn build_settle_request(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<reqwest::blocking::Request, X402FacilitatorTransportError> {
        validate_verify_wire_request(&self.config, request)?;
        let body = serde_json::json!({
            "x402Version": request.x402_version,
            "paymentPayload": request.payment_payload,
            "paymentRequirements": request.payment_requirements,
        });

        self.http_client()?
            .post(self.config.settle_url())
            .header(ACCEPT, "application/json")
            .header(
                AUTHORIZATION,
                self.credential_provider.authorization_header()?,
            )
            .json(&body)
            .build()
            .map_err(|_| {
                X402FacilitatorTransportError::InvalidConfiguration(
                    "facilitator settle request could not be constructed".to_string(),
                )
            })
    }

    fn execute_settle(
        &self,
        request: reqwest::blocking::Request,
    ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
        let response = self
            .http_client()?
            .execute(request)
            .map_err(map_reqwest_error)?;
        let status = response.status();
        ensure_settle_status(status)?;

        if response
            .content_length()
            .is_some_and(|length| length > MAX_FACILITATOR_RESPONSE_BYTES)
        {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "facilitator response exceeds the size limit".to_string(),
            ));
        }

        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let mut body = Vec::new();
        response
            .take(MAX_FACILITATOR_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut {
                    X402FacilitatorTransportError::Timeout
                } else {
                    X402FacilitatorTransportError::Unavailable(
                        "facilitator response body could not be read".to_string(),
                    )
                }
            })?;
        parse_settle_response(status, content_type.as_ref(), &body, &self.config.network)
    }
}

impl<P> fmt::Debug for ReqwestX402FacilitatorTransport<P>
where
    P: X402FacilitatorCredentialProvider,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReqwestX402FacilitatorTransport")
            .field("endpoint", &self.config.endpoint)
            .field("network", &self.config.network)
            .field("timeout_ms", &self.config.timeout_ms)
            .field(
                "credential_provider",
                &self.credential_provider.provider_kind(),
            )
            .field("credential", &"<redacted>")
            .finish()
    }
}

impl<P> X402FacilitatorTransport for ReqwestX402FacilitatorTransport<P>
where
    P: X402FacilitatorCredentialProvider,
{
    fn transport_kind(&self) -> &'static str {
        "authenticated_https_supported_verify_settle"
    }

    fn supported(&self) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError> {
        self.execute_supported(self.build_supported_request()?)
    }

    fn verify(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
        self.execute_verify(self.build_verify_request(request)?)
    }

    fn settle(
        &self,
        _authorization: X402SettlementAuthorization,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
        self.execute_settle(self.build_settle_request(request)?)
    }
}

fn map_reqwest_error(error: reqwest::Error) -> X402FacilitatorTransportError {
    if error.is_timeout() {
        X402FacilitatorTransportError::Timeout
    } else {
        X402FacilitatorTransportError::Unavailable("facilitator HTTPS request failed".to_string())
    }
}

fn ensure_supported_status(
    status: reqwest::StatusCode,
) -> Result<(), X402FacilitatorTransportError> {
    if status.is_success() {
        return Ok(());
    }
    if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator rejected the runtime credential".to_string(),
        ));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(X402FacilitatorTransportError::Unavailable(
            "facilitator is unavailable".to_string(),
        ));
    }
    Err(X402FacilitatorTransportError::InvalidResponse(format!(
        "facilitator returned unexpected HTTP status {}",
        status.as_u16()
    )))
}

fn ensure_verify_status(status: reqwest::StatusCode) -> Result<(), X402FacilitatorTransportError> {
    if status.is_success() {
        return Ok(());
    }
    if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator rejected the runtime credential".to_string(),
        ));
    }
    if status == reqwest::StatusCode::REQUEST_TIMEOUT {
        return Err(X402FacilitatorTransportError::Timeout);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(X402FacilitatorTransportError::Unavailable(
            "facilitator is unavailable".to_string(),
        ));
    }
    if status.is_client_error() {
        return Ok(());
    }
    Err(X402FacilitatorTransportError::InvalidResponse(format!(
        "facilitator returned unexpected HTTP status {}",
        status.as_u16()
    )))
}

fn ensure_settle_status(status: reqwest::StatusCode) -> Result<(), X402FacilitatorTransportError> {
    if status.is_success() {
        return Ok(());
    }
    if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator rejected the runtime credential".to_string(),
        ));
    }
    if status == reqwest::StatusCode::REQUEST_TIMEOUT {
        return Err(X402FacilitatorTransportError::Timeout);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(X402FacilitatorTransportError::Unavailable(
            "facilitator settlement outcome is unavailable".to_string(),
        ));
    }
    if status.is_client_error() {
        return Ok(());
    }
    Err(X402FacilitatorTransportError::InvalidResponse(format!(
        "facilitator returned unexpected HTTP status {}",
        status.as_u16()
    )))
}

fn validate_verify_wire_request(
    config: &X402FacilitatorConfig,
    request: &X402FacilitatorRequest,
) -> Result<(), X402FacilitatorTransportError> {
    if request.x402_version != X402_FACILITATOR_PROTOCOL_VERSION {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator verify request must use x402 v2".to_string(),
        ));
    }
    if request.network != config.network {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator verify request network must match configured network".to_string(),
        ));
    }
    let idempotency_key = request.idempotency_key.trim();
    if idempotency_key.is_empty() || idempotency_key.len() > 128 {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator verify idempotency key must contain 1-128 bytes".to_string(),
        ));
    }

    let payment_payload = request.payment_payload.as_object().ok_or_else(|| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "paymentPayload must be an object".to_string(),
        )
    })?;
    let payment_requirements = request.payment_requirements.as_object().ok_or_else(|| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "paymentRequirements must be an object".to_string(),
        )
    })?;

    if payment_payload.get("x402Version").and_then(Value::as_u64)
        != Some(u64::from(X402_FACILITATOR_PROTOCOL_VERSION))
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentPayload.x402Version must be 2".to_string(),
        ));
    }
    if payment_payload.get("accepted") != Some(&request.payment_requirements) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentPayload.accepted must exactly match paymentRequirements".to_string(),
        ));
    }
    if !payment_payload.get("payload").is_some_and(Value::is_object) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentPayload.payload must be an object".to_string(),
        ));
    }

    for (field, expected) in [
        ("scheme", "exact"),
        ("network", config.network.as_str()),
        ("asset", config.asset_contract.as_str()),
        ("payTo", config.receiver.as_str()),
    ] {
        if payment_requirements.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                format!("paymentRequirements.{field} does not match facilitator configuration"),
            ));
        }
    }

    let amount = payment_requirements
        .get("amount")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if amount.is_empty()
        || !amount.bytes().all(|byte| byte.is_ascii_digit())
        || amount.bytes().all(|byte| byte == b'0')
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentRequirements.amount must be a positive base-unit integer string".to_string(),
        ));
    }
    if payment_requirements
        .get("maxTimeoutSeconds")
        .and_then(Value::as_u64)
        .is_none_or(|timeout| timeout == 0)
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentRequirements.maxTimeoutSeconds must be a positive integer".to_string(),
        ));
    }
    if !payment_requirements
        .get("extra")
        .is_some_and(Value::is_object)
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "paymentRequirements.extra must be an object".to_string(),
        ));
    }
    Ok(())
}

fn parse_supported_response(
    content_type: Option<&HeaderValue>,
    body: &[u8],
) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError> {
    if body.len() as u64 > MAX_FACILITATOR_RESPONSE_BYTES {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response exceeds the size limit".to_string(),
        ));
    }
    let content_type = content_type
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    if content_type != "application/json" && !content_type.ends_with("+json") {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response must use a JSON content type".to_string(),
        ));
    }
    serde_json::from_slice(body).map_err(|_| {
        X402FacilitatorTransportError::InvalidResponse(
            "facilitator supported response is not valid x402 v2 JSON".to_string(),
        )
    })
}

fn parse_verify_response(
    status: reqwest::StatusCode,
    content_type: Option<&HeaderValue>,
    body: &[u8],
) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
    if body.len() as u64 > MAX_FACILITATOR_RESPONSE_BYTES {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response exceeds the size limit".to_string(),
        ));
    }
    let content_type = content_type
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    if content_type != "application/json" && !content_type.ends_with("+json") {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response must use a JSON content type".to_string(),
        ));
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VerifyWireResponse {
        is_valid: bool,
        #[serde(default)]
        invalid_reason: Option<String>,
    }

    let response: VerifyWireResponse = serde_json::from_slice(body).map_err(|_| {
        X402FacilitatorTransportError::InvalidResponse(
            "facilitator verify response is not valid x402 v2 JSON".to_string(),
        )
    })?;
    if !status.is_success() && response.is_valid {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator returned a valid payment with a failing HTTP status".to_string(),
        ));
    }

    Ok(X402FacilitatorVerifyResponse {
        is_valid: response.is_valid,
        invalid_reason: response.invalid_reason,
    })
}

fn parse_settle_response(
    status: reqwest::StatusCode,
    content_type: Option<&HeaderValue>,
    body: &[u8],
    expected_network: &str,
) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
    if body.len() as u64 > MAX_FACILITATOR_RESPONSE_BYTES {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response exceeds the size limit".to_string(),
        ));
    }
    let content_type = content_type
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    if content_type != "application/json" && !content_type.ends_with("+json") {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator response must use a JSON content type".to_string(),
        ));
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SettleWireResponse {
        success: bool,
        #[serde(default)]
        error_reason: Option<String>,
        transaction: String,
        network: String,
    }

    let response: SettleWireResponse = serde_json::from_slice(body).map_err(|_| {
        X402FacilitatorTransportError::InvalidResponse(
            "facilitator settle response is not valid x402 v2 JSON".to_string(),
        )
    })?;
    if !status.is_success() && response.success {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator returned a successful settlement with a failing HTTP status".to_string(),
        ));
    }
    if response.network != expected_network {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "facilitator settlement response network does not match the configured network"
                .to_string(),
        ));
    }

    let transaction = response.transaction.trim();
    let error_reason = response
        .error_reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty());
    if response.success {
        if transaction.len() != 64 || !transaction.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "successful settlement response requires a Stellar transaction hash".to_string(),
            ));
        }
        if error_reason.is_some() {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "successful settlement response cannot include an error reason".to_string(),
            ));
        }
    } else {
        if !transaction.is_empty() {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "rejected settlement response cannot include a transaction hash".to_string(),
            ));
        }
        if error_reason.is_none() {
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "rejected settlement response requires an error reason".to_string(),
            ));
        }
    }

    Ok(X402FacilitatorSettleResponse {
        success: response.success,
        transaction_hash: response.success.then(|| transaction.to_string()),
        error_reason: error_reason.map(str::to_string),
    })
}

pub trait X402FacilitatorTransport {
    fn transport_kind(&self) -> &'static str;
    fn supported(&self) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError>;
    fn verify(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError>;
    fn settle(
        &self,
        authorization: X402SettlementAuthorization,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError>;
}

/// Capability issued only after the persistent settlement state machine has
/// accepted a single settlement attempt.
///
/// External callers cannot construct this value in safe Rust, so they cannot
/// call the raw transport settlement method directly.
///
/// ```compile_fail
/// use neurochain::x402_facilitator::X402SettlementAuthorization;
///
/// let _authorization = X402SettlementAuthorization { _private: () };
/// ```
#[derive(Debug)]
pub struct X402SettlementAuthorization {
    _private: (),
}

impl X402SettlementAuthorization {
    fn after_persistent_begin() -> Self {
        Self { _private: () }
    }
}

pub fn verify_with_capability_handshake<T: X402FacilitatorTransport>(
    config: &X402FacilitatorConfig,
    transport: &T,
    request: &X402FacilitatorRequest,
) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
    if request.network != config.network {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator request network must match configured network".to_string(),
        ));
    }

    let supported = transport.supported()?;
    config.validate_supported(&supported)?;
    transport.verify(request)
}

pub fn settle_after_verified_request<T: X402FacilitatorTransport>(
    config: &X402FacilitatorConfig,
    transport: &T,
    store: &mut dyn X402ChallengeStore,
    challenge_id: &str,
    verified_request: &X402FacilitatorRequest,
    verification: &X402FacilitatorVerifyResponse,
    settle_request: &X402FacilitatorRequest,
) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
    if store.store_kind() != "file" {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator settlement requires the persistent file challenge store".to_string(),
        ));
    }
    if !verification.is_valid {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "rejected facilitator verification cannot proceed to settlement".to_string(),
        ));
    }
    if verified_request.network != config.network || settle_request.network != config.network {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "verified and settlement request networks must match configured network".to_string(),
        ));
    }
    if verified_request.idempotency_key != settle_request.idempotency_key
        || verified_request.payment_payload != settle_request.payment_payload
        || verified_request.payment_requirements != settle_request.payment_requirements
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "settlement request must exactly match the verified payment and idempotency key"
                .to_string(),
        ));
    }

    let request_digest = facilitator_request_digest(settle_request)?;
    match store
        .begin_settlement(challenge_id, &request_digest)
        .map_err(|error| {
            X402FacilitatorTransportError::Unavailable(format!(
                "x402 settlement state transition failed: {error}"
            ))
        })? {
        X402BeginSettlementOutcome::Started(_) => {}
        X402BeginSettlementOutcome::AlreadySettled(record) => {
            let transaction_hash = record.transaction_hash.ok_or_else(|| {
                X402FacilitatorTransportError::InvalidResponse(
                    "settled payment is missing its transaction hash".to_string(),
                )
            })?;
            return Ok(X402FacilitatorSettleResponse {
                success: true,
                transaction_hash: Some(transaction_hash),
                error_reason: None,
            });
        }
        X402BeginSettlementOutcome::AlreadyInProgress(record)
        | X402BeginSettlementOutcome::Blocked(record) => {
            return Err(X402FacilitatorTransportError::Unavailable(format!(
                "x402 settlement cannot be retried from state {}",
                record.state.as_str()
            )));
        }
        X402BeginSettlementOutcome::NotVerified => {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "payment must be persistently verified before settlement".to_string(),
            ));
        }
        X402BeginSettlementOutcome::BindingMismatch => {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "settlement request digest does not match verified payment".to_string(),
            ));
        }
        X402BeginSettlementOutcome::ReplayBlocked(_)
        | X402BeginSettlementOutcome::Expired(_)
        | X402BeginSettlementOutcome::UnknownChallenge => {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "settlement challenge is unavailable".to_string(),
            ));
        }
    }

    let response = match transport.settle(
        X402SettlementAuthorization::after_persistent_begin(),
        settle_request,
    ) {
        Ok(response) => response,
        Err(error) => {
            mark_settlement_outcome_unknown(store, challenge_id, &request_digest)?;
            return Err(error);
        }
    };

    let completion = if response.success {
        let Some(transaction_hash) = response
            .transaction_hash
            .as_deref()
            .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        else {
            mark_settlement_outcome_unknown(store, challenge_id, &request_digest)?;
            return Err(X402FacilitatorTransportError::InvalidResponse(
                "successful settlement response requires a Stellar transaction hash".to_string(),
            ));
        };
        X402SettlementCompletion::Settled {
            transaction_hash: transaction_hash.to_string(),
        }
    } else if response.transaction_hash.is_some() {
        mark_settlement_outcome_unknown(store, challenge_id, &request_digest)?;
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "rejected settlement response cannot include a transaction hash".to_string(),
        ));
    } else {
        X402SettlementCompletion::Rejected
    };

    match store
        .complete_settlement(challenge_id, &request_digest, completion)
        .map_err(|error| {
            X402FacilitatorTransportError::Unavailable(format!(
                "x402 settlement completion state failed: {error}"
            ))
        })? {
        X402CompleteSettlementOutcome::Completed(_) => Ok(response),
        X402CompleteSettlementOutcome::AlreadyCompleted(record) => {
            Ok(X402FacilitatorSettleResponse {
                success: true,
                transaction_hash: record.transaction_hash,
                error_reason: None,
            })
        }
        X402CompleteSettlementOutcome::StateConflict(_)
        | X402CompleteSettlementOutcome::NotVerified
        | X402CompleteSettlementOutcome::BindingMismatch
        | X402CompleteSettlementOutcome::UnknownChallenge => {
            Err(X402FacilitatorTransportError::Unavailable(
                "x402 settlement completion state is inconsistent".to_string(),
            ))
        }
    }
}

fn mark_settlement_outcome_unknown(
    store: &mut dyn X402ChallengeStore,
    challenge_id: &str,
    request_digest: &str,
) -> Result<(), X402FacilitatorTransportError> {
    match store
        .complete_settlement(
            challenge_id,
            request_digest,
            X402SettlementCompletion::OutcomeUnknown,
        )
        .map_err(|error| {
            X402FacilitatorTransportError::Unavailable(format!(
                "x402 uncertain settlement state failed: {error}"
            ))
        })? {
        X402CompleteSettlementOutcome::Completed(_) => Ok(()),
        X402CompleteSettlementOutcome::AlreadyCompleted(_)
        | X402CompleteSettlementOutcome::StateConflict(_)
        | X402CompleteSettlementOutcome::NotVerified
        | X402CompleteSettlementOutcome::BindingMismatch
        | X402CompleteSettlementOutcome::UnknownChallenge => {
            Err(X402FacilitatorTransportError::Unavailable(
                "x402 uncertain settlement state is inconsistent".to_string(),
            ))
        }
    }
}

pub fn facilitator_request_from_adapter(
    envelope: &Value,
    expected_operation: &str,
) -> Result<X402FacilitatorRequest, X402FacilitatorTransportError> {
    let operation = required_adapter_string(envelope, "operation")?;
    if operation != expected_operation {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            format!("expected adapter operation {expected_operation:?}, got {operation:?}"),
        ));
    }

    let payment_payload = envelope
        .get("payment_payload")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_payload object is required".to_string(),
            )
        })?;
    let payment_requirements = envelope
        .get("payment_requirements")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_requirements object is required".to_string(),
            )
        })?;
    let x402_version = payment_payload
        .get("x402Version")
        .or_else(|| payment_payload.get("x402_version"))
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_payload.x402Version must be an unsigned byte".to_string(),
            )
        })?;
    if x402_version != X402_FACILITATOR_PROTOCOL_VERSION {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            format!("unsupported x402 version {x402_version}; expected 2"),
        ));
    }

    let idempotency_key = required_adapter_string(envelope, "idempotency_key")?.to_string();
    let network = required_adapter_string(envelope, "network")?.to_string();
    if payment_requirements.get("network").and_then(Value::as_str) != Some(network.as_str()) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "adapter network must match payment_requirements.network".to_string(),
        ));
    }

    Ok(X402FacilitatorRequest {
        x402_version,
        payment_payload,
        payment_requirements,
        idempotency_key,
        network,
    })
}

pub fn verify_response_to_adapter(
    request: &X402FacilitatorRequest,
    response: &X402FacilitatorVerifyResponse,
) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "verify",
        "outcome": if response.is_valid { "verified" } else { "rejected" },
        "payment_payload": request.payment_payload,
        "payment_requirements": request.payment_requirements,
        "idempotency_key": request.idempotency_key,
        "network": request.network,
        "verification": {
            "is_valid": response.is_valid,
            "invalid_reason": response.invalid_reason,
        },
        "underlying_action_submit_allowed": false,
    })
}

fn verify_unavailable_to_adapter(
    request: &X402FacilitatorRequest,
    error: &X402FacilitatorTransportError,
) -> Value {
    let invalid_reason = match error {
        X402FacilitatorTransportError::InvalidConfiguration(_) => {
            "facilitator_configuration_invalid"
        }
        X402FacilitatorTransportError::Unavailable(_) => "facilitator_unavailable",
        X402FacilitatorTransportError::Timeout => "facilitator_timeout",
        X402FacilitatorTransportError::InvalidResponse(_) => "facilitator_invalid_response",
    };

    serde_json::json!({
        "schema_version": 1,
        "operation": "verify",
        "outcome": "unavailable",
        "payment_payload": request.payment_payload,
        "payment_requirements": request.payment_requirements,
        "idempotency_key": request.idempotency_key,
        "network": request.network,
        "verification": {
            "is_valid": false,
            "invalid_reason": invalid_reason,
        },
        "underlying_action_submit_allowed": false,
    })
}

pub struct X402FacilitatorVerifyOnlyAdapter<'a, T>
where
    T: X402FacilitatorTransport,
{
    config: X402FacilitatorConfig,
    transport: &'a T,
}

impl<'a, T> X402FacilitatorVerifyOnlyAdapter<'a, T>
where
    T: X402FacilitatorTransport,
{
    pub fn new(
        config: X402FacilitatorConfig,
        transport: &'a T,
    ) -> Result<Self, X402FacilitatorTransportError> {
        let config = X402FacilitatorConfig::validate(
            &config.endpoint,
            &config.network,
            &config.asset_contract,
            &config.receiver,
            config.timeout_ms,
        )?;
        Ok(Self { config, transport })
    }

    pub fn transport_kind(&self) -> &'static str {
        self.transport.transport_kind()
    }

    pub fn verify_adapter_envelope(
        &self,
        envelope: &Value,
    ) -> Result<Value, X402FacilitatorTransportError> {
        let request = facilitator_request_from_adapter(envelope, "verify")?;
        let response =
            match verify_with_capability_handshake(&self.config, self.transport, &request) {
                Ok(response) => verify_response_to_adapter(&request, &response),
                Err(error) => verify_unavailable_to_adapter(&request, &error),
            };
        Ok(response)
    }

    pub fn verify_request(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
        verify_with_capability_handshake(&self.config, self.transport, request)
    }
}

pub fn settle_response_to_adapter(
    request: &X402FacilitatorRequest,
    response: &X402FacilitatorSettleResponse,
) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "settle",
        "outcome": if response.success { "settled" } else { "rejected" },
        "payment_payload": request.payment_payload,
        "payment_requirements": request.payment_requirements,
        "idempotency_key": request.idempotency_key,
        "network": request.network,
        "verification": {
            "is_valid": true,
            "invalid_reason": null,
        },
        "settlement": {
            "success": response.success,
            "transaction_hash": response.transaction_hash,
            "error_reason": response.error_reason,
        },
        "underlying_action_submit_allowed": false,
    })
}

fn required_adapter_string<'a>(
    envelope: &'a Value,
    field: &str,
) -> Result<&'a str, X402FacilitatorTransportError> {
    envelope
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "{field} must be a non-empty string"
            ))
        })
}

fn required_runtime_env(name: &str) -> Result<String, X402FacilitatorTransportError> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "required facilitator runtime setting {name} is missing"
            ))
        })
}

fn optional_runtime_u64(name: &str, default: u64) -> Result<u64, X402FacilitatorTransportError> {
    match env::var(name) {
        Ok(value) => value.trim().parse::<u64>().map_err(|_| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "facilitator runtime setting {name} must be an unsigned integer"
            ))
        }),
        Err(_) => Ok(default),
    }
}

fn validate_resource_url(resource_url: &str) -> Result<(), X402FacilitatorTransportError> {
    let parsed = reqwest::Url::parse(resource_url).map_err(|_| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator resource URL is invalid".to_string(),
        )
    })?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator resource URL must be a credential-free HTTPS URL without a fragment"
                .to_string(),
        ));
    }
    Ok(())
}

fn encode_x402_header(value: &Value) -> Result<String, X402FacilitatorTransportError> {
    let bytes = serde_json::to_vec(value).map_err(|_| {
        X402FacilitatorTransportError::InvalidResponse(
            "x402 header JSON could not be encoded".to_string(),
        )
    })?;
    if bytes.len() > MAX_X402_HEADER_BYTES {
        return Err(X402FacilitatorTransportError::InvalidResponse(
            "x402 header JSON exceeds the size limit".to_string(),
        ));
    }
    Ok(BASE64_STANDARD.encode(bytes))
}

fn decode_x402_header(encoded: &str) -> Result<Value, X402FacilitatorTransportError> {
    let encoded = encoded.trim();
    if encoded.is_empty() || encoded.len() > MAX_X402_HEADER_ENCODED_BYTES {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE header is empty or exceeds the size limit".to_string(),
        ));
    }
    let bytes = BASE64_STANDARD.decode(encoded).map_err(|_| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE header is not valid Base64".to_string(),
        )
    })?;
    if bytes.len() > MAX_X402_HEADER_BYTES {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE payload exceeds the size limit".to_string(),
        ));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE payload is not valid JSON".to_string(),
        )
    })?;
    if !value.is_object() {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE payload must be an object".to_string(),
        ));
    }
    Ok(value)
}

fn facilitator_request_from_payment_signature(
    runtime: &X402FacilitatorRuntimeConfig,
    payment_signature: &str,
) -> Result<(String, X402FacilitatorRequest), X402FacilitatorTransportError> {
    let payment_payload = decode_x402_header(payment_signature)?;
    if payment_payload.get("x402Version").and_then(Value::as_u64)
        != Some(u64::from(X402_FACILITATOR_PROTOCOL_VERSION))
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE must contain an x402 v2 PaymentPayload".to_string(),
        ));
    }
    if !payment_payload.get("payload").is_some_and(Value::is_object) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE payload.payload must be an object".to_string(),
        ));
    }
    let challenge_id = payment_payload
        .pointer("/accepted/extra/neurochainChallengeId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "PAYMENT-SIGNATURE is missing a valid NeuroChain challenge binding".to_string(),
            )
        })?
        .to_string();
    let payment_requirements = runtime.payment_requirements(&challenge_id);
    if payment_payload.get("accepted") != Some(&payment_requirements) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE accepted requirements do not match the issued challenge".to_string(),
        ));
    }
    if payment_payload
        .get("resource")
        .is_some_and(|resource| resource != &runtime.resource())
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE resource does not match the protected resource".to_string(),
        ));
    }
    if payment_payload
        .get("extensions")
        .is_some_and(|extensions| !extensions.is_object())
    {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "PAYMENT-SIGNATURE extensions must be an object".to_string(),
        ));
    }

    Ok((
        challenge_id.clone(),
        X402FacilitatorRequest {
            x402_version: X402_FACILITATOR_PROTOCOL_VERSION,
            payment_payload,
            payment_requirements,
            idempotency_key: challenge_id,
            network: runtime.transport.network.clone(),
        },
    ))
}

fn is_stellar_strkey(value: &str, prefixes: &[char]) -> bool {
    value.len() == 56
        && value
            .chars()
            .next()
            .is_some_and(|first| prefixes.contains(&first))
        && value
            .chars()
            .all(|character| matches!(character, 'A'..='Z' | '2'..='7'))
}

#[derive(Debug, Clone)]
pub enum X402PaymentVerification {
    Finalized {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    ReplayBlocked {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    Expired {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    FacilitatorRejected {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    VerifiedPendingSettlement {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    SettlementStateUnavailable {
        challenge_id: String,
        challenge: X402StellarChallenge,
        payment_state: String,
    },
    InvalidPayment,
}

pub trait X402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str;
    fn boundary_kind(&self) -> &'static str;
    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String>;
    fn payment_required_presentation(
        &self,
        challenge_id: &str,
    ) -> Result<X402PaymentRequiredPresentation, String>;
    fn verify_payment(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String>;
}

#[derive(Debug, Default)]
struct MockX402PaymentVerifier;

struct FacilitatorX402PaymentVerifier<T>
where
    T: X402FacilitatorTransport,
{
    runtime: X402FacilitatorRuntimeConfig,
    transport: T,
}

#[derive(Debug)]
struct UnavailableX402PaymentVerifier {
    reason: String,
    verifier_kind: &'static str,
    boundary_kind: &'static str,
}

impl X402PaymentVerifier for MockX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "mock"
    }

    fn boundary_kind(&self) -> &'static str {
        "mock_header_store"
    }

    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        store.create_challenge()
    }

    fn payment_required_presentation(
        &self,
        challenge_id: &str,
    ) -> Result<X402PaymentRequiredPresentation, String> {
        Ok(X402PaymentRequiredPresentation::mock(challenge_id))
    }

    fn verify_payment(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        let Some(challenge_id) =
            mock_challenge_from_signature(payment_signature).map(str::to_string)
        else {
            return Ok(X402PaymentVerification::InvalidPayment);
        };

        let verification = match store.begin_finalize(&challenge_id)? {
            X402FinalizeOutcome::Finalized(challenge) => X402PaymentVerification::Finalized {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::ReplayBlocked(challenge) => {
                X402PaymentVerification::ReplayBlocked {
                    challenge_id,
                    challenge,
                }
            }
            X402FinalizeOutcome::Expired(challenge) => X402PaymentVerification::Expired {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::UnknownChallenge => X402PaymentVerification::InvalidPayment,
        };

        Ok(verification)
    }
}

impl<T> X402PaymentVerifier for FacilitatorX402PaymentVerifier<T>
where
    T: X402FacilitatorTransport + Send + Sync,
{
    fn verifier_kind(&self) -> &'static str {
        "facilitator"
    }

    fn boundary_kind(&self) -> &'static str {
        "facilitator_verify_only_pending_settlement"
    }

    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        if store.store_kind() != "file" {
            return Err(
                "facilitator x402 verifier requires the persistent file challenge store"
                    .to_string(),
            );
        }
        store.create_challenge()
    }

    fn payment_required_presentation(
        &self,
        challenge_id: &str,
    ) -> Result<X402PaymentRequiredPresentation, String> {
        self.runtime
            .payment_required_presentation(challenge_id)
            .map_err(|error| facilitator_error_code(&error).to_string())
    }

    fn verify_payment(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        if store.store_kind() != "file" {
            return Err(
                "facilitator x402 verifier requires the persistent file challenge store"
                    .to_string(),
            );
        }
        let (challenge_id, request) =
            match facilitator_request_from_payment_signature(&self.runtime, payment_signature) {
                Ok(request) => request,
                Err(_) => return Ok(X402PaymentVerification::InvalidPayment),
            };
        let request_digest = facilitator_request_digest(&request)
            .map_err(|error| facilitator_error_code(&error).to_string())?;
        let challenge = match store.inspect_challenge(&challenge_id)? {
            X402ChallengeInspection::Available(challenge) => challenge,
            X402ChallengeInspection::ReplayBlocked(challenge) => {
                return Ok(X402PaymentVerification::ReplayBlocked {
                    challenge_id,
                    challenge,
                });
            }
            X402ChallengeInspection::Expired(challenge) => {
                return Ok(X402PaymentVerification::Expired {
                    challenge_id,
                    challenge,
                });
            }
            X402ChallengeInspection::UnknownChallenge => {
                return Ok(X402PaymentVerification::InvalidPayment);
            }
        };
        match store.inspect_settlement(&challenge_id)? {
            X402SettlementInspection::NotVerified => {}
            X402SettlementInspection::Recorded(record) => {
                if record.request_digest != request_digest {
                    return Ok(X402PaymentVerification::InvalidPayment);
                }
                return if record.state == X402SettlementState::VerifiedPendingSettlement {
                    Ok(X402PaymentVerification::VerifiedPendingSettlement {
                        challenge_id,
                        challenge,
                    })
                } else {
                    Ok(X402PaymentVerification::SettlementStateUnavailable {
                        challenge_id,
                        challenge,
                        payment_state: record.state.as_str().to_string(),
                    })
                };
            }
            X402SettlementInspection::UnknownChallenge => {
                return Ok(X402PaymentVerification::InvalidPayment);
            }
        }

        let adapter =
            X402FacilitatorVerifyOnlyAdapter::new(self.runtime.transport.clone(), &self.transport)
                .map_err(|error| facilitator_error_code(&error).to_string())?;
        let verification = adapter
            .verify_request(&request)
            .map_err(|error| facilitator_error_code(&error).to_string())?;
        if verification.is_valid {
            match store.record_verified_payment(&challenge_id, &request_digest)? {
                X402RecordVerificationOutcome::Recorded(_)
                | X402RecordVerificationOutcome::AlreadyRecorded(_) => {
                    Ok(X402PaymentVerification::VerifiedPendingSettlement {
                        challenge_id,
                        challenge,
                    })
                }
                X402RecordVerificationOutcome::SettlementBlocked(record) => {
                    Ok(X402PaymentVerification::SettlementStateUnavailable {
                        challenge_id,
                        challenge,
                        payment_state: record.state.as_str().to_string(),
                    })
                }
                X402RecordVerificationOutcome::ReplayBlocked(challenge) => {
                    Ok(X402PaymentVerification::ReplayBlocked {
                        challenge_id,
                        challenge,
                    })
                }
                X402RecordVerificationOutcome::Expired(challenge) => {
                    Ok(X402PaymentVerification::Expired {
                        challenge_id,
                        challenge,
                    })
                }
                X402RecordVerificationOutcome::BindingMismatch
                | X402RecordVerificationOutcome::UnknownChallenge => {
                    Ok(X402PaymentVerification::InvalidPayment)
                }
            }
        } else {
            Ok(X402PaymentVerification::FacilitatorRejected {
                challenge_id,
                challenge,
            })
        }
    }
}

impl X402PaymentVerifier for UnavailableX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        self.verifier_kind
    }

    fn boundary_kind(&self) -> &'static str {
        self.boundary_kind
    }

    fn create_challenge(
        &self,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        Err(self.reason.clone())
    }

    fn payment_required_presentation(
        &self,
        _challenge_id: &str,
    ) -> Result<X402PaymentRequiredPresentation, String> {
        Err(self.reason.clone())
    }

    fn verify_payment(
        &self,
        _payment_signature: &str,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        Err(self.reason.clone())
    }
}

pub fn build_x402_payment_verifier() -> Box<dyn X402PaymentVerifier + Send + Sync> {
    let mode = env::var("NC_X402_STELLAR_VERIFIER")
        .unwrap_or_else(|_| "mock".to_string())
        .trim()
        .to_ascii_lowercase();

    select_x402_payment_verifier(&mode, x402_runtime_is_production())
}

fn select_x402_payment_verifier(
    mode: &str,
    production_runtime: bool,
) -> Box<dyn X402PaymentVerifier + Send + Sync> {
    match mode {
        "mock" if production_runtime => Box::new(UnavailableX402PaymentVerifier {
            reason:
                "mock x402 verifier is disabled in production; configure the facilitator verifier"
                    .to_string(),
            verifier_kind: "unavailable",
            boundary_kind: "facilitator_required",
        }),
        "mock" => Box::<MockX402PaymentVerifier>::default(),
        "facilitator" => build_facilitator_payment_verifier(),
        _ => Box::new(UnavailableX402PaymentVerifier {
            reason: format!(
                "unsupported x402 verifier mode {mode:?}; expected \"mock\" or \"facilitator\""
            ),
            verifier_kind: "unavailable",
            boundary_kind: "facilitator_required",
        }),
    }
}

fn build_facilitator_payment_verifier() -> Box<dyn X402PaymentVerifier + Send + Sync> {
    let runtime = match X402FacilitatorRuntimeConfig::from_env() {
        Ok(runtime) => runtime,
        Err(error) => {
            return Box::new(UnavailableX402PaymentVerifier {
                reason: facilitator_error_code(&error).to_string(),
                verifier_kind: "facilitator",
                boundary_kind: "facilitator_verify_only_pending_settlement",
            });
        }
    };
    let transport = match ReqwestX402FacilitatorTransport::new(
        runtime.transport.clone(),
        EnvX402FacilitatorCredentialProvider::default(),
    ) {
        Ok(transport) => transport,
        Err(error) => {
            return Box::new(UnavailableX402PaymentVerifier {
                reason: facilitator_error_code(&error).to_string(),
                verifier_kind: "facilitator",
                boundary_kind: "facilitator_verify_only_pending_settlement",
            });
        }
    };
    Box::new(FacilitatorX402PaymentVerifier { runtime, transport })
}

fn facilitator_error_code(error: &X402FacilitatorTransportError) -> &'static str {
    match error {
        X402FacilitatorTransportError::InvalidConfiguration(_) => {
            "facilitator_configuration_invalid"
        }
        X402FacilitatorTransportError::Unavailable(_) => "facilitator_unavailable",
        X402FacilitatorTransportError::Timeout => "facilitator_timeout",
        X402FacilitatorTransportError::InvalidResponse(_) => "facilitator_invalid_response",
    }
}

fn mock_challenge_from_signature(signature: &str) -> Option<&str> {
    signature
        .trim()
        .strip_prefix("paid:")
        .map(str::trim)
        .filter(|challenge_id| !challenge_id.is_empty())
}

fn x402_runtime_is_production() -> bool {
    ["NC_ENV", "APP_ENV", "RUST_ENV"].iter().any(|key| {
        env::var(key)
            .ok()
            .map(|value| value.trim().eq_ignore_ascii_case("production"))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct TestCredentialProvider {
        authorization: HeaderValue,
    }

    impl TestCredentialProvider {
        fn new(value: &'static str) -> Self {
            let mut authorization = HeaderValue::from_static(value);
            authorization.set_sensitive(true);
            Self { authorization }
        }
    }

    impl fmt::Debug for TestCredentialProvider {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("TestCredentialProvider(<redacted>)")
        }
    }

    impl X402FacilitatorCredentialProvider for TestCredentialProvider {
        fn provider_kind(&self) -> &'static str {
            "test"
        }

        fn authorization_header(&self) -> Result<HeaderValue, X402FacilitatorTransportError> {
            Ok(self.authorization.clone())
        }
    }

    fn authenticated_test_config() -> X402FacilitatorConfig {
        X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap()
    }

    fn runtime_test_config() -> X402FacilitatorRuntimeConfig {
        X402FacilitatorRuntimeConfig {
            transport: authenticated_test_config(),
            amount: "10000".to_string(),
            max_timeout_seconds: 60,
            resource_url: "https://stellarzerolab.com/api/x402/stellar/intent-plan".to_string(),
        }
    }

    fn encoded_test_payment_payload(
        runtime: &X402FacilitatorRuntimeConfig,
        challenge_id: &str,
    ) -> String {
        encode_x402_header(&serde_json::json!({
            "x402Version": 2,
            "accepted": runtime.payment_requirements(challenge_id),
            "payload": { "transaction": "offline-fixture-xdr" },
            "resource": runtime.resource(),
            "extensions": {},
        }))
        .unwrap()
    }

    fn official_verify_request() -> X402FacilitatorRequest {
        let payment_requirements = serde_json::json!({
            "scheme": "exact",
            "network": "stellar:testnet",
            "asset": "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "amount": "10000",
            "payTo": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "maxTimeoutSeconds": 60,
            "extra": { "areFeesSponsored": true }
        });
        X402FacilitatorRequest {
            x402_version: X402_FACILITATOR_PROTOCOL_VERSION,
            payment_payload: serde_json::json!({
                "x402Version": 2,
                "accepted": payment_requirements,
                "payload": { "transaction": "offline-fixture-xdr" }
            }),
            payment_requirements,
            idempotency_key: "offline-verify-request-0001".to_string(),
            network: "stellar:testnet".to_string(),
        }
    }

    #[derive(Debug)]
    struct FakeFacilitatorTransport {
        calls: Mutex<Vec<&'static str>>,
        supported_error: Option<X402FacilitatorTransportError>,
        verify_response: X402FacilitatorVerifyResponse,
        verify_error: Option<X402FacilitatorTransportError>,
        settle_error: Option<X402FacilitatorTransportError>,
    }

    impl FakeFacilitatorTransport {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: None,
                verify_response: X402FacilitatorVerifyResponse {
                    is_valid: true,
                    invalid_reason: None,
                },
                verify_error: None,
                settle_error: None,
            }
        }

        fn failing_supported(error: X402FacilitatorTransportError) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: Some(error),
                verify_response: X402FacilitatorVerifyResponse {
                    is_valid: true,
                    invalid_reason: None,
                },
                verify_error: None,
                settle_error: None,
            }
        }

        fn rejecting_verify(reason: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: None,
                verify_response: X402FacilitatorVerifyResponse {
                    is_valid: false,
                    invalid_reason: Some(reason.to_string()),
                },
                verify_error: None,
                settle_error: None,
            }
        }

        fn failing_verify(error: X402FacilitatorTransportError) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: None,
                verify_response: X402FacilitatorVerifyResponse {
                    is_valid: false,
                    invalid_reason: None,
                },
                verify_error: Some(error),
                settle_error: None,
            }
        }

        fn failing_settle(error: X402FacilitatorTransportError) -> Self {
            let mut transport = Self::new();
            transport.settle_error = Some(error);
            transport
        }
    }

    impl X402FacilitatorTransport for FakeFacilitatorTransport {
        fn transport_kind(&self) -> &'static str {
            "offline_fake"
        }

        fn supported(
            &self,
        ) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("supported");
            if let Some(error) = &self.supported_error {
                return Err(error.clone());
            }
            Ok(serde_json::from_str(include_str!(
                "../examples/x402_facilitator_adapter/supported_stellar_exact_v2.json"
            ))
            .unwrap())
        }

        fn verify(
            &self,
            request: &X402FacilitatorRequest,
        ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("verify");
            if request.x402_version != X402_FACILITATOR_PROTOCOL_VERSION {
                return Err(X402FacilitatorTransportError::InvalidConfiguration(
                    "unsupported x402 version".to_string(),
                ));
            }
            if let Some(error) = &self.verify_error {
                return Err(error.clone());
            }
            Ok(self.verify_response.clone())
        }

        fn settle(
            &self,
            _authorization: X402SettlementAuthorization,
            request: &X402FacilitatorRequest,
        ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("settle");
            if request.idempotency_key.trim().is_empty() {
                return Err(X402FacilitatorTransportError::InvalidConfiguration(
                    "idempotency key is required".to_string(),
                ));
            }
            if let Some(error) = &self.settle_error {
                return Err(error.clone());
            }
            Ok(X402FacilitatorSettleResponse {
                success: true,
                transaction_hash: Some("a".repeat(64)),
                error_reason: None,
            })
        }
    }

    #[derive(Debug, Default)]
    struct TestChallengeStore {
        created: bool,
        finalized: bool,
        persistent: bool,
        verified_request_digest: Option<String>,
        settlement_state: Option<X402SettlementState>,
        settlement_transaction_hash: Option<String>,
    }

    impl TestChallengeStore {
        fn settlement_record(&self) -> Option<crate::x402_store::X402SettlementRecord> {
            Some(crate::x402_store::X402SettlementRecord {
                request_digest: self.verified_request_digest.clone()?,
                state: self
                    .settlement_state
                    .clone()
                    .unwrap_or(X402SettlementState::VerifiedPendingSettlement),
                verified_at: 2,
                settlement_started_at: self.settlement_state.as_ref().and_then(|state| {
                    (*state != X402SettlementState::VerifiedPendingSettlement).then_some(3)
                }),
                settlement_completed_at: self.settlement_state.as_ref().and_then(|state| {
                    matches!(
                        state,
                        X402SettlementState::Settled
                            | X402SettlementState::SettlementRejected
                            | X402SettlementState::SettlementOutcomeUnknown
                    )
                    .then_some(4)
                }),
                transaction_hash: self.settlement_transaction_hash.clone(),
            })
        }
    }

    impl X402ChallengeStore for TestChallengeStore {
        fn store_kind(&self) -> &'static str {
            if self.persistent {
                "file"
            } else {
                "test"
            }
        }

        fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
            self.created = true;
            Ok(X402ChallengeRecord {
                challenge_id: "x402s0001".to_string(),
                challenge: X402StellarChallenge {
                    created_at: 1,
                    expires_at: 300,
                    finalized: false,
                    finalized_at: None,
                    payment_state: "payment_required".to_string(),
                },
            })
        }

        fn inspect_challenge(&self, challenge_id: &str) -> Result<X402ChallengeInspection, String> {
            if challenge_id == "x402s0001" {
                Ok(X402ChallengeInspection::Available(X402StellarChallenge {
                    created_at: 1,
                    expires_at: u64::MAX,
                    finalized: false,
                    finalized_at: None,
                    payment_state: "payment_required".to_string(),
                }))
            } else {
                Ok(X402ChallengeInspection::UnknownChallenge)
            }
        }

        fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
            self.finalized = true;
            if challenge_id == "x402s0001" {
                Ok(X402FinalizeOutcome::Finalized(X402StellarChallenge {
                    created_at: 1,
                    expires_at: 300,
                    finalized: true,
                    finalized_at: Some(2),
                    payment_state: "finalized".to_string(),
                }))
            } else {
                Ok(X402FinalizeOutcome::UnknownChallenge)
            }
        }

        fn inspect_settlement(
            &self,
            challenge_id: &str,
        ) -> Result<X402SettlementInspection, String> {
            if challenge_id == "x402s0001" {
                Ok(match self.settlement_record() {
                    Some(record) => X402SettlementInspection::Recorded(record),
                    None => X402SettlementInspection::NotVerified,
                })
            } else {
                Ok(X402SettlementInspection::UnknownChallenge)
            }
        }

        fn record_verified_payment(
            &mut self,
            challenge_id: &str,
            request_digest: &str,
        ) -> Result<X402RecordVerificationOutcome, String> {
            if challenge_id != "x402s0001" || request_digest.len() != 64 {
                return Ok(X402RecordVerificationOutcome::UnknownChallenge);
            }
            self.verified_request_digest = Some(request_digest.to_string());
            self.settlement_state = Some(X402SettlementState::VerifiedPendingSettlement);
            self.settlement_transaction_hash = None;
            Ok(X402RecordVerificationOutcome::Recorded(
                self.settlement_record().expect("recorded test settlement"),
            ))
        }

        fn begin_settlement(
            &mut self,
            challenge_id: &str,
            request_digest: &str,
        ) -> Result<crate::x402_store::X402BeginSettlementOutcome, String> {
            if challenge_id != "x402s0001" {
                return Ok(X402BeginSettlementOutcome::UnknownChallenge);
            }
            let Some(record) = self.settlement_record() else {
                return Ok(X402BeginSettlementOutcome::NotVerified);
            };
            if record.request_digest != request_digest {
                return Ok(X402BeginSettlementOutcome::BindingMismatch);
            }
            Ok(match record.state {
                X402SettlementState::VerifiedPendingSettlement => {
                    self.settlement_state = Some(X402SettlementState::SettlementInProgress);
                    X402BeginSettlementOutcome::Started(
                        self.settlement_record().expect("started test settlement"),
                    )
                }
                X402SettlementState::SettlementInProgress => {
                    X402BeginSettlementOutcome::AlreadyInProgress(record)
                }
                X402SettlementState::Settled => X402BeginSettlementOutcome::AlreadySettled(record),
                X402SettlementState::SettlementRejected
                | X402SettlementState::SettlementOutcomeUnknown => {
                    X402BeginSettlementOutcome::Blocked(record)
                }
            })
        }

        fn complete_settlement(
            &mut self,
            challenge_id: &str,
            request_digest: &str,
            completion: crate::x402_store::X402SettlementCompletion,
        ) -> Result<crate::x402_store::X402CompleteSettlementOutcome, String> {
            if challenge_id != "x402s0001" {
                return Ok(X402CompleteSettlementOutcome::UnknownChallenge);
            }
            let Some(record) = self.settlement_record() else {
                return Ok(X402CompleteSettlementOutcome::NotVerified);
            };
            if record.request_digest != request_digest {
                return Ok(X402CompleteSettlementOutcome::BindingMismatch);
            }
            if record.state == X402SettlementState::Settled {
                return Ok(X402CompleteSettlementOutcome::AlreadyCompleted(record));
            }
            if record.state != X402SettlementState::SettlementInProgress {
                return Ok(X402CompleteSettlementOutcome::StateConflict(record));
            }

            match completion {
                X402SettlementCompletion::Settled { transaction_hash } => {
                    self.settlement_state = Some(X402SettlementState::Settled);
                    self.settlement_transaction_hash = Some(transaction_hash);
                    self.finalized = true;
                }
                X402SettlementCompletion::Rejected => {
                    self.settlement_state = Some(X402SettlementState::SettlementRejected);
                }
                X402SettlementCompletion::OutcomeUnknown => {
                    self.settlement_state = Some(X402SettlementState::SettlementOutcomeUnknown);
                }
            }
            Ok(X402CompleteSettlementOutcome::Completed(
                self.settlement_record().expect("completed test settlement"),
            ))
        }
    }

    #[test]
    fn selects_mock_verifier_only_for_non_production_runtime() {
        let verifier = select_x402_payment_verifier("mock", false);
        assert_eq!(verifier.verifier_kind(), "mock");
        assert_eq!(verifier.boundary_kind(), "mock_header_store");

        let mut store = TestChallengeStore::default();
        let challenge = verifier.create_challenge(&mut store).unwrap();
        assert_eq!(challenge.challenge_id, "x402s0001");
        assert!(store.created);

        let verification = verifier
            .verify_payment("paid:x402s0001", &mut store)
            .unwrap();
        assert!(matches!(
            verification,
            X402PaymentVerification::Finalized { .. }
        ));
        assert!(store.finalized);
    }

    #[test]
    fn disables_mock_verifier_for_production_runtime() {
        let verifier = select_x402_payment_verifier("mock", true);
        assert_eq!(verifier.verifier_kind(), "unavailable");
        assert_eq!(verifier.boundary_kind(), "facilitator_required");

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert!(err.contains("mock x402 verifier is disabled in production"));
        assert!(!store.created);

        let err = verifier
            .verify_payment("paid:x402s0001", &mut store)
            .unwrap_err();
        assert!(err.contains("configure the facilitator verifier"));
        assert!(!store.finalized);
    }

    #[test]
    fn facilitator_selection_fails_closed_without_runtime_configuration() {
        let verifier = select_x402_payment_verifier("facilitator", false);
        assert_eq!(verifier.verifier_kind(), "facilitator");
        assert_eq!(
            verifier.boundary_kind(),
            "facilitator_verify_only_pending_settlement"
        );

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert_eq!(err, "facilitator_configuration_invalid");
        assert!(!store.created);

        let err = verifier
            .verify_payment("paid:x402s0001", &mut store)
            .unwrap_err();
        assert_eq!(err, "facilitator_configuration_invalid");
        assert!(!store.finalized);
    }

    #[test]
    fn facilitator_payment_required_header_is_x402_v2_and_challenge_bound() {
        let runtime = runtime_test_config();
        let presentation = runtime.payment_required_presentation("x402s0001").unwrap();
        let decoded = decode_x402_header(presentation.encoded_header.as_deref().unwrap()).unwrap();

        assert_eq!(decoded["x402Version"], 2);
        assert_eq!(
            decoded["resource"]["url"],
            "https://stellarzerolab.com/api/x402/stellar/intent-plan"
        );
        assert_eq!(decoded["accepts"][0]["scheme"], "exact");
        assert_eq!(decoded["accepts"][0]["network"], "stellar:testnet");
        assert_eq!(
            decoded["accepts"][0]["extra"]["neurochainChallengeId"],
            "x402s0001"
        );
        assert_eq!(
            decoded["extensions"]["neurochain"]["underlyingActionSubmitAllowed"],
            false
        );
        assert!(presentation.mock_signature.is_none());
    }

    #[test]
    fn facilitator_verify_only_runtime_never_finalizes_or_settles() {
        let runtime = runtime_test_config();
        let signature = encoded_test_payment_payload(&runtime, "x402s0001");
        let verifier = FacilitatorX402PaymentVerifier {
            runtime,
            transport: FakeFacilitatorTransport::new(),
        };
        let mut store = TestChallengeStore {
            persistent: true,
            ..Default::default()
        };

        let verification = verifier.verify_payment(&signature, &mut store).unwrap();
        let repeated = verifier.verify_payment(&signature, &mut store).unwrap();

        assert!(matches!(
            verification,
            X402PaymentVerification::VerifiedPendingSettlement { .. }
        ));
        assert!(matches!(
            repeated,
            X402PaymentVerification::VerifiedPendingSettlement { .. }
        ));
        assert!(!store.finalized);
        assert_eq!(
            verifier.transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );
    }

    #[test]
    fn facilitator_request_digest_is_stable_across_object_field_order() {
        let first = official_verify_request();
        let mut second = first.clone();
        second.payment_requirements = serde_json::json!({
            "extra": first.payment_requirements["extra"],
            "maxTimeoutSeconds": first.payment_requirements["maxTimeoutSeconds"],
            "payTo": first.payment_requirements["payTo"],
            "amount": first.payment_requirements["amount"],
            "asset": first.payment_requirements["asset"],
            "network": first.payment_requirements["network"],
            "scheme": first.payment_requirements["scheme"],
        });
        second.payment_payload["accepted"] = second.payment_requirements.clone();

        assert_eq!(
            facilitator_request_digest(&first).unwrap(),
            facilitator_request_digest(&second).unwrap()
        );

        second.idempotency_key.push_str("-changed");
        assert_ne!(
            facilitator_request_digest(&first).unwrap(),
            facilitator_request_digest(&second).unwrap()
        );
    }

    #[test]
    fn facilitator_verify_only_runtime_maps_rejection_and_malformed_header_fail_closed() {
        let runtime = runtime_test_config();
        let signature = encoded_test_payment_payload(&runtime, "x402s0001");
        let verifier = FacilitatorX402PaymentVerifier {
            runtime,
            transport: FakeFacilitatorTransport::rejecting_verify("fixture_rejected"),
        };
        let mut store = TestChallengeStore {
            persistent: true,
            ..Default::default()
        };

        assert!(matches!(
            verifier.verify_payment(&signature, &mut store).unwrap(),
            X402PaymentVerification::FacilitatorRejected { .. }
        ));
        assert!(!store.finalized);
        assert!(matches!(
            verifier
                .verify_payment("paid:x402s0001", &mut store)
                .unwrap(),
            X402PaymentVerification::InvalidPayment
        ));
        assert_eq!(
            verifier.transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );
    }

    #[test]
    fn unsupported_verifier_mode_is_unavailable() {
        let verifier = select_x402_payment_verifier("wallet", false);
        assert_eq!(verifier.verifier_kind(), "unavailable");
        assert_eq!(verifier.boundary_kind(), "facilitator_required");

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert!(err.contains("unsupported x402 verifier mode"));
        assert!(err.contains("mock"));
        assert!(err.contains("facilitator"));
        assert!(!store.created);
    }

    #[test]
    fn facilitator_transport_port_keeps_verify_and_settle_separate_offline() {
        let transport = FakeFacilitatorTransport::new();
        assert_eq!(transport.transport_kind(), "offline_fake");
        let request = X402FacilitatorRequest {
            x402_version: X402_FACILITATOR_PROTOCOL_VERSION,
            payment_payload: serde_json::json!({"payload_ref": "fixture:payment"}),
            payment_requirements: serde_json::json!({
                "scheme": "exact",
                "network": "stellar:testnet"
            }),
            idempotency_key: "fixture-request-0001".to_string(),
            network: "stellar:testnet".to_string(),
        };

        let verified = transport.verify(&request).unwrap();
        assert!(verified.is_valid);
        assert_eq!(transport.calls.lock().unwrap().as_slice(), ["verify"]);

        let settled = transport
            .settle(
                X402SettlementAuthorization::after_persistent_begin(),
                &request,
            )
            .unwrap();
        assert!(settled.success);
        assert_eq!(
            settled.transaction_hash.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["verify", "settle"]
        );
    }

    #[test]
    fn facilitator_transport_port_rejects_invalid_version_and_idempotency() {
        let transport = FakeFacilitatorTransport::new();
        let mut request = X402FacilitatorRequest {
            x402_version: 1,
            payment_payload: serde_json::json!({"payload_ref": "fixture:payment"}),
            payment_requirements: serde_json::json!({"scheme": "exact"}),
            idempotency_key: "fixture-request-0001".to_string(),
            network: "stellar:testnet".to_string(),
        };

        assert!(matches!(
            transport.verify(&request),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        request.x402_version = X402_FACILITATOR_PROTOCOL_VERSION;
        request.idempotency_key.clear();
        assert!(matches!(
            transport.settle(
                X402SettlementAuthorization::after_persistent_begin(),
                &request,
            ),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn adapter_fixtures_round_trip_through_offline_transport() {
        let transport = FakeFacilitatorTransport::new();
        let verify_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let verify_request = facilitator_request_from_adapter(&verify_fixture, "verify").unwrap();
        let verify_response = transport.verify(&verify_request).unwrap();
        assert_eq!(
            verify_response_to_adapter(&verify_request, &verify_response),
            verify_fixture
        );

        let settle_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/settle_success.json"
        ))
        .unwrap();
        let settle_request = facilitator_request_from_adapter(&settle_fixture, "settle").unwrap();
        let settle_response = transport
            .settle(
                X402SettlementAuthorization::after_persistent_begin(),
                &settle_request,
            )
            .unwrap();
        assert_eq!(
            settle_response_to_adapter(&settle_request, &settle_response),
            settle_fixture
        );
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["verify", "settle"]
        );
    }

    #[test]
    fn adapter_mapping_rejects_operation_and_network_mismatch() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        assert!(matches!(
            facilitator_request_from_adapter(&fixture, "settle"),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        let mut mismatched = fixture;
        mismatched["network"] = Value::String("stellar:pubnet".to_string());
        assert!(matches!(
            facilitator_request_from_adapter(&mismatched, "verify"),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn facilitator_config_validates_stellar_transport_without_network_calls() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet/",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();

        assert_eq!(
            config.endpoint,
            "https://channels.openzeppelin.com/x402/testnet"
        );
        assert_eq!(
            config.verify_url(),
            "https://channels.openzeppelin.com/x402/testnet/verify"
        );
        assert_eq!(
            config.settle_url(),
            "https://channels.openzeppelin.com/x402/testnet/settle"
        );
        assert_eq!(
            config.supported_url(),
            "https://channels.openzeppelin.com/x402/testnet/supported"
        );
    }

    #[test]
    fn facilitator_config_rejects_unsafe_or_incomplete_values() {
        let valid_asset = "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA";
        let valid_receiver = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        for (endpoint, network, asset, receiver, timeout_ms) in [
            (
                "http://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://user:secret@facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example?network=testnet",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:mainnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                "USDC",
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                "receiver",
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                50,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                60_000,
            ),
        ] {
            assert!(matches!(
                X402FacilitatorConfig::validate(endpoint, network, asset, receiver, timeout_ms),
                Err(X402FacilitatorTransportError::InvalidConfiguration(_))
            ));
        }
    }

    #[test]
    fn facilitator_capability_handshake_accepts_configured_stellar_network_offline() {
        let transport = FakeFacilitatorTransport::new();
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let supported = transport.supported().unwrap();

        config.validate_supported(&supported).unwrap();
        assert_eq!(transport.calls.lock().unwrap().as_slice(), ["supported"]);
    }

    #[test]
    fn facilitator_capability_handshake_fails_closed_on_mismatch() {
        let config = X402FacilitatorConfig::validate(
            "https://facilitator.example",
            "stellar:pubnet",
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let testnet_only: X402FacilitatorSupportedResponse = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/supported_stellar_exact_v2.json"
        ))
        .unwrap();

        assert!(matches!(
            config.validate_supported(&testnet_only),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        let unsupported_version = X402FacilitatorSupportedResponse {
            kinds: vec![X402FacilitatorSupportedKind {
                x402_version: 1,
                scheme: "exact".to_string(),
                network: config.network.clone(),
                extra: Value::Object(Default::default()),
            }],
            signers: BTreeMap::new(),
            extensions: Vec::new(),
        };
        assert!(matches!(
            config.validate_supported(&unsupported_version),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn authenticated_supported_request_is_built_offline_with_sensitive_credential() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let transport = ReqwestX402FacilitatorTransport::new(
            config,
            TestCredentialProvider::new("Bearer test-only-placeholder"),
        )
        .unwrap();

        let request = transport.build_supported_request().unwrap();
        let authorization = request.headers().get(AUTHORIZATION).unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(
            request.url().as_str(),
            "https://channels.openzeppelin.com/x402/testnet/supported"
        );
        assert_eq!(authorization, "Bearer test-only-placeholder");
        assert!(authorization.is_sensitive());
        assert!(!format!("{transport:?}").contains("test-only-placeholder"));
        assert!(!format!("{request:?}").contains("test-only-placeholder"));
    }

    #[test]
    fn authenticated_verify_request_matches_official_v2_wire_shape_offline() {
        let transport = ReqwestX402FacilitatorTransport::new(
            authenticated_test_config(),
            TestCredentialProvider::new("Bearer test-only-placeholder"),
        )
        .unwrap();
        let request = official_verify_request();

        let built = transport.build_verify_request(&request).unwrap();
        let authorization = built.headers().get(AUTHORIZATION).unwrap();
        let body: Value =
            serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();

        assert_eq!(built.method(), reqwest::Method::POST);
        assert_eq!(
            built.url().as_str(),
            "https://channels.openzeppelin.com/x402/testnet/verify"
        );
        assert_eq!(built.headers().get(ACCEPT).unwrap(), "application/json");
        assert_eq!(
            built.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(authorization, "Bearer test-only-placeholder");
        assert!(authorization.is_sensitive());
        assert_eq!(body["x402Version"], 2);
        assert_eq!(body["paymentPayload"], request.payment_payload);
        assert_eq!(body["paymentRequirements"], request.payment_requirements);
        assert!(body.get("idempotencyKey").is_none());
        assert!(!format!("{built:?}").contains("test-only-placeholder"));
    }

    #[test]
    fn authenticated_verify_request_validation_fails_closed_offline() {
        let config = authenticated_test_config();
        let valid = official_verify_request();

        let mut wrong_version = valid.clone();
        wrong_version.payment_payload["x402Version"] = Value::from(1);
        let mut wrong_asset = valid.clone();
        wrong_asset.payment_requirements["asset"] =
            Value::String("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".to_string());
        wrong_asset.payment_payload["accepted"] = wrong_asset.payment_requirements.clone();
        let mut mismatched_accepted = valid.clone();
        mismatched_accepted.payment_payload["accepted"]["amount"] =
            Value::String("9999".to_string());
        let mut missing_payload = valid.clone();
        missing_payload
            .payment_payload
            .as_object_mut()
            .unwrap()
            .remove("payload");
        let mut empty_idempotency = valid;
        empty_idempotency.idempotency_key.clear();

        for invalid in [
            wrong_version,
            wrong_asset,
            mismatched_accepted,
            missing_payload,
            empty_idempotency,
        ] {
            assert!(matches!(
                validate_verify_wire_request(&config, &invalid),
                Err(X402FacilitatorTransportError::InvalidConfiguration(_))
            ));
        }
    }

    #[test]
    fn authenticated_transport_revalidates_manually_constructed_config() {
        let config = X402FacilitatorConfig {
            endpoint: "http://facilitator.example".to_string(),
            network: "stellar:testnet".to_string(),
            asset_contract: "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA".to_string(),
            receiver: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
            timeout_ms: 5_000,
        };

        assert!(matches!(
            ReqwestX402FacilitatorTransport::new(
                config,
                TestCredentialProvider::new("Bearer test-only-placeholder"),
            ),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn authenticated_supported_response_parsing_is_bounded_and_fail_closed() {
        let content_type = HeaderValue::from_static("application/json; charset=utf-8");
        let response = parse_supported_response(
            Some(&content_type),
            include_bytes!("../examples/x402_facilitator_adapter/supported_stellar_exact_v2.json"),
        )
        .unwrap();
        assert_eq!(response.kinds[0].x402_version, 2);
        assert_eq!(response.kinds[0].network, "stellar:testnet");
        assert_eq!(
            response.signers["stellar:testnet"],
            ["GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"]
        );

        assert!(matches!(
            parse_supported_response(
                Some(&HeaderValue::from_static("text/html")),
                br#"{"kinds":[]}"#
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_supported_response(
                Some(&HeaderValue::from_static("application/json")),
                &vec![b' '; MAX_FACILITATOR_RESPONSE_BYTES as usize + 1],
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            ensure_supported_status(reqwest::StatusCode::UNAUTHORIZED),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(matches!(
            ensure_supported_status(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));
    }

    #[test]
    fn authenticated_verify_response_preserves_valid_and_rejected_outcomes_offline() {
        let content_type = HeaderValue::from_static("application/json; charset=utf-8");
        let verified = parse_verify_response(
            reqwest::StatusCode::OK,
            Some(&content_type),
            br#"{"isValid":true,"payer":"G-PAYER","extensions":{}}"#,
        )
        .unwrap();
        assert!(verified.is_valid);
        assert_eq!(verified.invalid_reason, None);

        let rejected = parse_verify_response(
            reqwest::StatusCode::OK,
            Some(&content_type),
            br#"{"isValid":false,"invalidReason":"invalid_exact_stellar_payload_wrong_amount","payer":"G-PAYER"}"#,
        )
        .unwrap();
        assert!(!rejected.is_valid);
        assert_eq!(
            rejected.invalid_reason.as_deref(),
            Some("invalid_exact_stellar_payload_wrong_amount")
        );

        let client_rejection = parse_verify_response(
            reqwest::StatusCode::BAD_REQUEST,
            Some(&content_type),
            br#"{"isValid":false,"invalidReason":"invalid_exact_payload_malformed"}"#,
        )
        .unwrap();
        assert!(!client_rejection.is_valid);
    }

    #[test]
    fn authenticated_verify_response_errors_fail_closed_offline() {
        let json = HeaderValue::from_static("application/json");
        assert!(matches!(
            ensure_verify_status(reqwest::StatusCode::UNAUTHORIZED),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert_eq!(
            ensure_verify_status(reqwest::StatusCode::REQUEST_TIMEOUT),
            Err(X402FacilitatorTransportError::Timeout)
        );
        assert!(matches!(
            ensure_verify_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));
        assert!(matches!(
            ensure_verify_status(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));
        assert!(matches!(
            parse_verify_response(
                reqwest::StatusCode::OK,
                Some(&HeaderValue::from_static("text/html")),
                br#"{"isValid":true}"#,
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_verify_response(
                reqwest::StatusCode::OK,
                Some(&json),
                &vec![b' '; MAX_FACILITATOR_RESPONSE_BYTES as usize + 1],
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_verify_response(reqwest::StatusCode::OK, Some(&json), br#"{}"#),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_verify_response(
                reqwest::StatusCode::BAD_REQUEST,
                Some(&json),
                br#"{"isValid":true}"#,
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
    }

    #[test]
    fn authenticated_settle_response_preserves_success_and_rejection_offline() {
        let content_type = HeaderValue::from_static("application/json; charset=utf-8");
        let transaction = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let settled = parse_settle_response(
            reqwest::StatusCode::OK,
            Some(&content_type),
            format!(
                r#"{{"success":true,"transaction":"{transaction}","network":"stellar:testnet","payer":"G-PAYER"}}"#
            )
            .as_bytes(),
            "stellar:testnet",
        )
        .unwrap();
        assert!(settled.success);
        assert_eq!(settled.transaction_hash.as_deref(), Some(transaction));
        assert_eq!(settled.error_reason, None);

        let rejected = parse_settle_response(
            reqwest::StatusCode::BAD_REQUEST,
            Some(&content_type),
            br#"{"success":false,"errorReason":"invalid_exact_stellar_payload_wrong_amount","transaction":"","network":"stellar:testnet"}"#,
            "stellar:testnet",
        )
        .unwrap();
        assert!(!rejected.success);
        assert_eq!(rejected.transaction_hash, None);
        assert_eq!(
            rejected.error_reason.as_deref(),
            Some("invalid_exact_stellar_payload_wrong_amount")
        );
    }

    #[test]
    fn authenticated_settle_response_errors_fail_closed_offline() {
        let json = HeaderValue::from_static("application/json");
        assert!(matches!(
            ensure_settle_status(reqwest::StatusCode::UNAUTHORIZED),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert_eq!(
            ensure_settle_status(reqwest::StatusCode::REQUEST_TIMEOUT),
            Err(X402FacilitatorTransportError::Timeout)
        );
        assert!(matches!(
            ensure_settle_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));
        assert!(matches!(
            ensure_settle_status(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));

        for body in [
            br#"{}"#.as_slice(),
            br#"{"success":true,"transaction":"","network":"stellar:testnet"}"#.as_slice(),
            br#"{"success":true,"errorReason":"contradiction","transaction":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","network":"stellar:testnet"}"#.as_slice(),
            br#"{"success":false,"transaction":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","network":"stellar:testnet","errorReason":"rejected"}"#.as_slice(),
            br#"{"success":false,"transaction":"","network":"stellar:testnet"}"#.as_slice(),
            br#"{"success":false,"transaction":"","network":"stellar:pubnet","errorReason":"rejected"}"#.as_slice(),
        ] {
            assert!(matches!(
                parse_settle_response(
                    reqwest::StatusCode::OK,
                    Some(&json),
                    body,
                    "stellar:testnet"
                ),
                Err(X402FacilitatorTransportError::InvalidResponse(_))
            ));
        }
        assert!(matches!(
            parse_settle_response(
                reqwest::StatusCode::OK,
                Some(&HeaderValue::from_static("text/html")),
                br#"{"success":false,"transaction":"","network":"stellar:testnet","errorReason":"rejected"}"#,
                "stellar:testnet",
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_settle_response(
                reqwest::StatusCode::BAD_REQUEST,
                Some(&json),
                br#"{"success":true,"transaction":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","network":"stellar:testnet"}"#,
                "stellar:testnet",
            ),
            Err(X402FacilitatorTransportError::InvalidResponse(_))
        ));
    }

    #[test]
    #[ignore = "requires an explicit credential-bearing Stellar testnet network probe"]
    fn authenticated_supported_live_testnet_probe() {
        assert_eq!(
            env::var("NC_X402_LIVE_SUPPORTED_PROBE").as_deref(),
            Ok("1"),
            "set NC_X402_LIVE_SUPPORTED_PROBE=1 only for an explicitly approved live probe"
        );

        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            10_000,
        )
        .unwrap();
        let transport = ReqwestX402FacilitatorTransport::new(
            config.clone(),
            EnvX402FacilitatorCredentialProvider::default(),
        )
        .unwrap();

        let supported = transport.supported().unwrap();

        config.validate_supported(&supported).unwrap();
        assert_eq!(
            transport.transport_kind(),
            "authenticated_https_supported_verify_settle"
        );
    }

    #[test]
    #[ignore = "requires an explicitly approved authenticated Stellar testnet verify probe"]
    fn authenticated_verify_live_testnet_rejection_probe() {
        assert_eq!(
            env::var("NC_X402_LIVE_VERIFY_PROBE").as_deref(),
            Ok("1"),
            "set NC_X402_LIVE_VERIFY_PROBE=1 only for an explicitly approved live probe"
        );

        let transport = ReqwestX402FacilitatorTransport::new(
            authenticated_test_config(),
            EnvX402FacilitatorCredentialProvider::default(),
        )
        .unwrap();

        let response = transport.verify(&official_verify_request()).unwrap();

        assert!(
            !response.is_valid,
            "the deliberately malformed, unsigned fixture must be rejected"
        );
        let invalid_reason = response
            .invalid_reason
            .as_deref()
            .expect("a rejected live verify response must include an invalid reason");
        assert!(
            invalid_reason.starts_with("invalid_"),
            "unexpected live verify rejection reason: {invalid_reason}"
        );
        assert_eq!(
            transport.transport_kind(),
            "authenticated_https_supported_verify_settle"
        );
        println!("live x402 verify rejected safely: {invalid_reason}");
    }

    #[test]
    fn authenticated_settle_request_matches_official_v2_wire_shape_offline() {
        let transport = ReqwestX402FacilitatorTransport::new(
            authenticated_test_config(),
            TestCredentialProvider::new("Bearer test-only-placeholder"),
        )
        .unwrap();
        let request = official_verify_request();

        let built = transport.build_settle_request(&request).unwrap();
        let authorization = built.headers().get(AUTHORIZATION).unwrap();
        let body: Value =
            serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();

        assert_eq!(built.method(), reqwest::Method::POST);
        assert_eq!(
            built.url().as_str(),
            "https://channels.openzeppelin.com/x402/testnet/settle"
        );
        assert_eq!(built.headers().get(ACCEPT).unwrap(), "application/json");
        assert_eq!(
            built.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(authorization, "Bearer test-only-placeholder");
        assert!(authorization.is_sensitive());
        assert_eq!(body["x402Version"], 2);
        assert_eq!(body["paymentPayload"], request.payment_payload);
        assert_eq!(body["paymentRequirements"], request.payment_requirements);
        assert!(body.get("idempotencyKey").is_none());
        assert!(!format!("{built:?}").contains("test-only-placeholder"));
    }

    #[test]
    fn environment_credential_provider_exposes_only_source_metadata() {
        let provider = EnvX402FacilitatorCredentialProvider::default();
        assert_eq!(provider.variable(), X402_FACILITATOR_API_KEY_ENV);
        assert_eq!(provider.provider_kind(), "environment");
        assert!(format!("{provider:?}").contains("<runtime-only>"));
        assert!(matches!(
            EnvX402FacilitatorCredentialProvider::new(""),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn facilitator_verify_requires_successful_capability_handshake() {
        let transport = FakeFacilitatorTransport::new();
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let request = facilitator_request_from_adapter(&fixture, "verify").unwrap();

        let response = verify_with_capability_handshake(&config, &transport, &request).unwrap();

        assert!(response.is_valid);
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );
    }

    #[test]
    fn facilitator_verify_fails_closed_before_verify_on_handshake_errors() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let request = facilitator_request_from_adapter(&fixture, "verify").unwrap();

        for error in [
            X402FacilitatorTransportError::Unavailable(
                "offline facilitator unavailable".to_string(),
            ),
            X402FacilitatorTransportError::Timeout,
        ] {
            let transport = FakeFacilitatorTransport::failing_supported(error.clone());
            assert_eq!(
                verify_with_capability_handshake(&config, &transport, &request),
                Err(error)
            );
            assert_eq!(transport.calls.lock().unwrap().as_slice(), ["supported"]);
        }

        let mut mismatched_request = request;
        mismatched_request.network = "stellar:pubnet".to_string();
        let transport = FakeFacilitatorTransport::new();
        assert!(matches!(
            verify_with_capability_handshake(&config, &transport, &mismatched_request),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn verify_only_adapter_maps_offline_results_without_settlement() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();

        let accepted_transport = FakeFacilitatorTransport::new();
        let accepted_adapter =
            X402FacilitatorVerifyOnlyAdapter::new(config.clone(), &accepted_transport).unwrap();
        assert_eq!(accepted_adapter.transport_kind(), "offline_fake");
        let accepted = accepted_adapter.verify_adapter_envelope(&fixture).unwrap();
        assert_eq!(accepted["outcome"], "verified");
        assert_eq!(accepted["verification"]["is_valid"], true);
        assert_eq!(accepted["underlying_action_submit_allowed"], false);
        assert!(accepted.get("settlement").is_none());
        assert_eq!(
            accepted_transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );

        let rejected_transport = FakeFacilitatorTransport::rejecting_verify("facilitator_rejected");
        let rejected_adapter =
            X402FacilitatorVerifyOnlyAdapter::new(config.clone(), &rejected_transport).unwrap();
        let rejected = rejected_adapter.verify_adapter_envelope(&fixture).unwrap();
        assert_eq!(rejected["outcome"], "rejected");
        assert_eq!(rejected["verification"]["is_valid"], false);
        assert_eq!(
            rejected["verification"]["invalid_reason"],
            "facilitator_rejected"
        );
        assert_eq!(rejected["underlying_action_submit_allowed"], false);
        assert_eq!(
            rejected_transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );

        let timeout_transport =
            FakeFacilitatorTransport::failing_supported(X402FacilitatorTransportError::Timeout);
        let timeout_adapter =
            X402FacilitatorVerifyOnlyAdapter::new(config.clone(), &timeout_transport).unwrap();
        let timeout = timeout_adapter.verify_adapter_envelope(&fixture).unwrap();
        assert_eq!(timeout["outcome"], "unavailable");
        assert_eq!(timeout["verification"]["is_valid"], false);
        assert_eq!(
            timeout["verification"]["invalid_reason"],
            "facilitator_timeout"
        );
        assert_eq!(timeout["underlying_action_submit_allowed"], false);
        assert_eq!(
            timeout_transport.calls.lock().unwrap().as_slice(),
            ["supported"]
        );

        let unavailable_transport =
            FakeFacilitatorTransport::failing_verify(X402FacilitatorTransportError::Unavailable(
                "offline facilitator unavailable".to_string(),
            ));
        let unavailable_adapter =
            X402FacilitatorVerifyOnlyAdapter::new(config.clone(), &unavailable_transport).unwrap();
        let unavailable = unavailable_adapter
            .verify_adapter_envelope(&fixture)
            .unwrap();
        assert_eq!(unavailable["outcome"], "unavailable");
        assert_eq!(unavailable["verification"]["is_valid"], false);
        assert_eq!(
            unavailable["verification"]["invalid_reason"],
            "facilitator_unavailable"
        );
        assert_eq!(unavailable["underlying_action_submit_allowed"], false);
        assert_eq!(
            unavailable_transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );

        let malformed_transport = FakeFacilitatorTransport::new();
        let malformed_adapter =
            X402FacilitatorVerifyOnlyAdapter::new(config, &malformed_transport).unwrap();
        let mut malformed = fixture;
        malformed.as_object_mut().unwrap().remove("payment_payload");
        assert!(matches!(
            malformed_adapter.verify_adapter_envelope(&malformed),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(malformed_transport.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn facilitator_settle_requires_matching_successful_verification() {
        let transport = FakeFacilitatorTransport::new();
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let verify_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let settle_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/settle_success.json"
        ))
        .unwrap();
        let verify_request = facilitator_request_from_adapter(&verify_fixture, "verify").unwrap();
        let settle_request = facilitator_request_from_adapter(&settle_fixture, "settle").unwrap();
        let verification =
            verify_with_capability_handshake(&config, &transport, &verify_request).unwrap();
        let mut store = TestChallengeStore {
            persistent: true,
            ..TestChallengeStore::default()
        };
        let request_digest = facilitator_request_digest(&settle_request).unwrap();
        store
            .record_verified_payment("x402s0001", &request_digest)
            .unwrap();

        let settlement = settle_after_verified_request(
            &config,
            &transport,
            &mut store,
            "x402s0001",
            &verify_request,
            &verification,
            &settle_request,
        )
        .unwrap();

        assert!(settlement.success);
        assert!(store.finalized);
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify", "settle"]
        );

        let repeated = settle_after_verified_request(
            &config,
            &transport,
            &mut store,
            "x402s0001",
            &verify_request,
            &verification,
            &settle_request,
        )
        .unwrap();
        assert!(repeated.success);
        assert_eq!(
            repeated.transaction_hash.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify", "settle"]
        );
    }

    #[test]
    fn facilitator_settle_fails_closed_before_transport_on_rejection_or_mismatch() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let verified_request = facilitator_request_from_adapter(&fixture, "verify").unwrap();
        let accepted = X402FacilitatorVerifyResponse {
            is_valid: true,
            invalid_reason: None,
        };
        let rejected = X402FacilitatorVerifyResponse {
            is_valid: false,
            invalid_reason: Some("fixture rejected".to_string()),
        };

        let transport = FakeFacilitatorTransport::new();
        let mut non_persistent_store = TestChallengeStore::default();
        let non_persistent_error = settle_after_verified_request(
            &config,
            &transport,
            &mut non_persistent_store,
            "x402s0001",
            &verified_request,
            &accepted,
            &verified_request,
        )
        .unwrap_err();
        assert!(matches!(
            non_persistent_error,
            X402FacilitatorTransportError::InvalidConfiguration(message)
                if message.contains("persistent file challenge store")
        ));
        assert!(transport.calls.lock().unwrap().is_empty());

        let mut store = TestChallengeStore {
            persistent: true,
            ..TestChallengeStore::default()
        };
        assert!(matches!(
            settle_after_verified_request(
                &config,
                &transport,
                &mut store,
                "x402s0001",
                &verified_request,
                &rejected,
                &verified_request,
            ),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(transport.calls.lock().unwrap().is_empty());

        let mut mismatched = verified_request.clone();
        mismatched.idempotency_key = "different-idempotency-key".to_string();
        assert!(matches!(
            settle_after_verified_request(
                &config,
                &transport,
                &mut store,
                "x402s0001",
                &verified_request,
                &accepted,
                &mismatched,
            ),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(transport.calls.lock().unwrap().is_empty());

        let request_digest = facilitator_request_digest(&verified_request).unwrap();
        store
            .record_verified_payment("x402s0001", &request_digest)
            .unwrap();
        let timeout_transport =
            FakeFacilitatorTransport::failing_settle(X402FacilitatorTransportError::Timeout);
        assert_eq!(
            settle_after_verified_request(
                &config,
                &timeout_transport,
                &mut store,
                "x402s0001",
                &verified_request,
                &accepted,
                &verified_request,
            ),
            Err(X402FacilitatorTransportError::Timeout)
        );
        assert!(matches!(
            store.inspect_settlement("x402s0001").unwrap(),
            X402SettlementInspection::Recorded(crate::x402_store::X402SettlementRecord {
                state: X402SettlementState::SettlementOutcomeUnknown,
                ..
            })
        ));
        assert!(matches!(
            settle_after_verified_request(
                &config,
                &timeout_transport,
                &mut store,
                "x402s0001",
                &verified_request,
                &accepted,
                &verified_request,
            ),
            Err(X402FacilitatorTransportError::Unavailable(_))
        ));
        assert_eq!(
            timeout_transport.calls.lock().unwrap().as_slice(),
            ["settle"]
        );
    }
}
