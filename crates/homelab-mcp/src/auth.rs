use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

pub struct Auth {}

pub async fn require_cf_jwt(request: Request, next: Next) -> Result<Response, StatusCode> {
    let token = request
        .headers()
        .get("Cf-Access-Jwt-Assertion")
        .and_then(|value| value.to_str().ok());

    match token {
        Some(token) if verify_token(token) => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn verify_token(token: &str) -> bool {
    // TODO: this is the part we still need to fill in —
    // decode + check signature against Cloudflare's public key
    let header = decode_header(&token);
    let token_message = decode::<Claims>(
        &token,
        &DecodingKey::from_secret("test".as_ref()),
        &Validation::new(header.unwrap().alg),
    );

    todo!()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
}
