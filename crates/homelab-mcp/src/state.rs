use std::sync::Arc;

use crate::auth::AuthKeys;

/// Shared state reachable from any handler/middleware via `State<Arc<AppState>>`.
pub struct AppState {
    /// Absent for the local profile, where JWT validation is deliberately bypassed.
    pub auth_keys: Option<Arc<AuthKeys>>,
    pub profile: String,
}
