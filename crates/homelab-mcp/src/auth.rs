use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

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
    State(auth_keys): State<Arc<AuthKeys>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
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

    match verify_token(token, auth_keys.as_ref()) {
        Ok(()) => Ok(next.run(request).await),
        Err(reason) => {
            tracing::warn!(?reason, "jwt rejected");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

fn verify_token(token: &str, auth_keys: &AuthKeys) -> Result<(), AuthFailure> {
    let header = decode_header(&token).map_err(|_| AuthFailure::HeaderDecodeFailed)?;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
}

#[derive(Debug)]
enum AuthFailure {
    MissingHeader,
    HeaderDecodeFailed,
    NoMatchingKey,
    KeyDecodeFailed,
    ValidationFailed,
}
