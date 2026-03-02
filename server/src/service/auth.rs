use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use tonic::{Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use crate::codegen::weyvelength::{
    LoginRequest, LoginResponse, RegisterRequest, RegisterResponse,
};
use crate::state::SharedState;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

const TOKEN_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7 days

pub async fn handle_register(
    state: &SharedState,
    req: RegisterRequest,
) -> Result<Response<RegisterResponse>, Status> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(Status::invalid_argument("Username and password must not be empty"));
    }

    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|e| Status::internal(e.to_string()))?
        .to_string();

    sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(&req.username)
        .bind(&hash)
        .execute(&state.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                Status::already_exists("Username is already taken")
            } else {
                Status::internal(e.to_string())
            }
        })?;

    println!("[auth] registered {}", req.username);
    Ok(Response::new(RegisterResponse {}))
}

pub async fn handle_login(
    state: &SharedState,
    req: LoginRequest,
) -> Result<Response<LoginResponse>, Status> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT password_hash FROM users WHERE username = ?")
            .bind(&req.username)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

    let (hash,) = row.ok_or_else(|| Status::unauthenticated("Invalid username or password"))?;

    let parsed = PasswordHash::new(&hash).map_err(|e| Status::internal(e.to_string()))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .map_err(|_| Status::unauthenticated("Invalid username or password"))?;

    let token = Uuid::new_v4().to_string();
    let expires_at = now_ms() + TOKEN_TTL_MS;

    sqlx::query("INSERT INTO auth_tokens (token, username, expires_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(&req.username)
        .bind(expires_at)
        .execute(&state.db)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    println!("[auth] login {}", req.username);
    Ok(Response::new(LoginResponse { token }))
}

/// Validates the `authorization: bearer <token>` metadata header.
/// Returns the username associated with the token, or `UNAUTHENTICATED`.
pub async fn validate_token(db: &sqlx::SqlitePool, meta: &MetadataMap) -> Result<String, Status> {
    let header = meta
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing authorization header"))?;

    let token = header
        .strip_prefix("bearer ")
        .ok_or_else(|| Status::unauthenticated("Invalid authorization header"))?;

    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT username, expires_at FROM auth_tokens WHERE token = ?")
            .bind(token)
            .fetch_optional(db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

    let (username, expires_at) =
        row.ok_or_else(|| Status::unauthenticated("Invalid or expired token"))?;

    if expires_at < now_ms() {
        return Err(Status::unauthenticated("Token has expired"));
    }

    Ok(username)
}
