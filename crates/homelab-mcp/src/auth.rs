use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct AuthKeys {
    pub keys: Vec<Key>,
}

#[derive(Deserialize)]
pub struct Key {
    pub kid: String,
    pub n: String,
    pub e: String,
}

pub async fn require_cf_jwt(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.profile == "local" {
        return Ok(next.run(request).await);
    };
    let token = request
        .headers()
        .get("Cf-Access-Jwt-Assertion")
        .and_then(|v| v.to_str().ok());

    let token = match token {
        Some(t) => t,
        None => {
            tracing::warn!(reason = ?AuthFailure::MissingHeader, "jwt rejected");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let auth_keys = state.auth_keys.as_ref().ok_or_else(|| {
        tracing::error!("Cloudflare keys are unavailable outside the local profile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match verify_token(token, auth_keys.as_ref()) {
        Ok(()) => Ok(next.run(request).await),
        Err(reason) => {
            tracing::warn!(?reason, "jwt rejected");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

fn verify_token(token: &str, auth_keys: &AuthKeys) -> Result<(), AuthFailure> {
    let header = decode_header(token).map_err(|_| AuthFailure::HeaderDecodeFailed)?;

    let key = auth_keys
        .keys
        .iter()
        .find(|k| Some(&k.kid) == header.kid.as_ref())
        .ok_or(AuthFailure::NoMatchingKey)?;

    let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e)
        .map_err(|_| AuthFailure::KeyDecodeFailed)?;

    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[std::env::var("CF_AUD").expect("CF_AUD not set")]); //sets AUD id
    decode::<Claims>(&token, &decoding_key, &validation)
        .map_err(|_| AuthFailure::ValidationFailed)?;

    Ok(())
}

// Empty on purpose: `decode` needs a target type to deserialize the token's
// claims into, but registered-claim checks (exp/aud/nbf/iss) run internally
// against the raw payload regardless of this type, and we don't read any
// claims ourselves — only whether decode succeeds or fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {}

#[derive(Debug)]
enum AuthFailure {
    MissingHeader,
    HeaderDecodeFailed,
    NoMatchingKey,
    KeyDecodeFailed,
    ValidationFailed,
}
