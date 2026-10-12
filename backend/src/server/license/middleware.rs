use crate::server::{config::AppState, shared::types::api::ApiError};
use axum::{
    body::Body,
    extract::State,
    http::{Method, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use super::online::ENTITLEMENT_PATH;
use crate::server::auth::middleware::billing::settings_route;
use crate::server::email::handlers::{SNS_FEEDBACK_PATH, UNSUBSCRIBE_PATH_PREFIX};

/// Requests the guard lets through whatever the license status is.
///
/// - Safe methods (GET, HEAD, OPTIONS) — read-only access
/// - Auth endpoints — users must be able to log in to see the banner
/// - Config endpoint — frontend needs this to display the locked banner
/// - Health endpoint — uptime monitors must still work
/// - The cloud entitlement endpoint — minting an entitlement for a self-hosted
///   instance is not a licensed feature, and blocking it deadlocks any server
///   that serves its own entitlements: it could never answer the request that
///   clears its own `Pending` status.
/// - One-click unsubscribes and SES feedback — turning off email a recipient
///   no longer wants is not a licensed feature.
/// - The Settings routes a lapsed cloud org keeps: renaming and deleting the
///   org, and editing the current user.
fn always_allowed(method: &Method, path: &str) -> bool {
    method.is_safe()
        || settings_route(method, path)
        || path.starts_with("/api/auth/")
        || path == "/api/config"
        || path == "/api/health"
        || path == ENTITLEMENT_PATH
        || path.starts_with(UNSUBSCRIBE_PATH_PREFIX)
        || path == SNS_FEEDBACK_PATH
}

/// Middleware that blocks mutating requests when the license is locked.
///
/// This is simpler than `demo_mode_middleware` — no auth introspection needed.
/// License state is global (not per-org), so we just check the service status.
pub async fn license_guard_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if always_allowed(request.method(), request.uri().path()) {
        return next.run(request).await;
    }

    // Check license status. No license service => no key configured
    // (community/cloud) => never locked.
    if let Some(license_service) = &state.license_service
        && license_service.current_status().await.is_locked()
    {
        return ApiError::license_locked().into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_auth_survive_a_lock() {
        assert!(always_allowed(&Method::GET, "/api/hosts"));
        assert!(always_allowed(&Method::POST, "/api/auth/login"));
        assert!(always_allowed(&Method::GET, "/api/config"));
        assert!(always_allowed(&Method::GET, "/api/health"));
    }

    #[test]
    fn writes_are_blocked_by_a_lock() {
        assert!(!always_allowed(&Method::POST, "/api/hosts"));
        assert!(!always_allowed(&Method::DELETE, "/api/sites/some-id"));
    }

    #[test]
    fn org_settings_survive_a_lock() {
        let org = format!("/api/v1/organizations/{}", uuid::Uuid::nil());
        let user = format!("/api/v1/users/{}", uuid::Uuid::nil());
        assert!(always_allowed(&Method::DELETE, &org));
        assert!(always_allowed(&Method::PUT, &org));
        assert!(always_allowed(&Method::PUT, &user));
        assert!(!always_allowed(&Method::DELETE, &user));
        assert!(!always_allowed(&Method::POST, &format!("{org}/reset")));
    }

    #[test]
    fn entitlement_requests_survive_a_lock() {
        // A server that serves entitlements has to answer this POST even while
        // its own license is locked, or it can never clear its own `Pending`.
        assert!(always_allowed(&Method::POST, ENTITLEMENT_PATH));
    }

    #[test]
    fn unsubscribes_and_email_feedback_survive_a_lock() {
        let unsubscribe = format!(
            "{UNSUBSCRIBE_PATH_PREFIX}{}/discovery_digest",
            uuid::Uuid::nil()
        );
        assert!(always_allowed(&Method::POST, &unsubscribe));
        assert!(always_allowed(&Method::POST, SNS_FEEDBACK_PATH));
    }
}
