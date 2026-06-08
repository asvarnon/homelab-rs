use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::head,
};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AuthKeys {
    pub keys: Vec<Key>,
}

#[derive(Deserialize)]
pub struct Key {
    pub kid: String,
    pub kty: String,
    pub alg: String,
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
        .and_then(|value| value.to_str().ok());

    match token {
        Some(token) if verify_token(token, auth_keys.as_ref()) => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn verify_token(token: &str, auth_keys: &AuthKeys) -> bool {
    let Ok(header) = decode_header(&token) else {
        // decoding header
        return false;
    };
    let Some(key) = auth_keys //matching fetched keys against decoded header key
        .keys
        .iter()
        .find(|k| Some(&k.kid) == header.kid.as_ref())
    else {
        return false;
    };

    let Ok(decoding_key) = DecodingKey::from_rsa_components(&key.n, &key.e) else {
        return false;
    };

    decode::<Claims>(&token, &decoding_key, &Validation::new(header.alg)).is_ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
}
