use std::sync::Arc;

use crate::auth::AuthKeys;

/// Shared state reachable from any handler/middleware via `State<Arc<AppState>>`.
pub struct AppState {
    pub auth_keys: Arc<AuthKeys>,
    pub profile: String,
}
