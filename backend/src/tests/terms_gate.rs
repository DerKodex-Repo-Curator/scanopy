//! The terms re-acceptance gate against a real database: which requests a
//! user who has to accept the terms is refused, and that accepting lets them in.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::middleware::{self, Next};
use axum::routing::{get, post};
use chrono::{Duration, NaiveTime, Utc};
use uuid::Uuid;

use super::self_hosted_licensing::{create_org, test_state_with_email_dir};
use super::user;
use crate::server::auth::middleware::auth::AuthenticatedEntity;
use crate::server::auth::middleware::terms::terms_gate_middleware;
use crate::server::auth::terms_gate::{TERMS_ACCEPTANCE_EFFECTIVE, acceptance_required};
use crate::server::billing::plans::get_enterprise_plan;
use crate::server::billing::types::base::{BillingPlan, PlanConfig};
use crate::server::shared::services::traits::CrudService;
use crate::server::users::r#impl::base::User;
use crate::server::users::r#impl::permissions::UserOrgPermissions;

fn session(user: &User) -> AuthenticatedEntity {
    AuthenticatedEntity::User {
        user_id: user.id,
        organization_id: user.base.organization_id,
        permissions: UserOrgPermissions::Owner,
        site_ids: vec![],
        email: user.base.email.clone(),
        email_verified: true,
        terms_accepted_at: user.base.terms_accepted_at,
    }
}

async fn status(addr: std::net::SocketAddr, method: reqwest::Method, path: &str) -> (u16, String) {
    let response = reqwest::Client::new()
        .request(method, format!("http://{addr}{path}"))
        .send()
        .await
        .unwrap();
    (response.status().as_u16(), response.text().await.unwrap())
}

#[tokio::test]
async fn a_user_who_accepted_earlier_terms_is_refused_until_they_accept_again() {
    let (state, _container) = test_state_with_email_dir(None).await;
    let org = create_org(&state, get_enterprise_plan(), None).await;
    let demo_org = create_org(&state, BillingPlan::Demo(PlanConfig::default()), None).await;

    let before_effective = TERMS_ACCEPTANCE_EFFECTIVE
        .and_time(NaiveTime::MIN)
        .and_utc()
        - Duration::days(1);
    let mut stale = user(&org.id);
    stale.base.terms_accepted_at = Some(before_effective);
    let stale = state
        .services
        .user_service
        .create(stale, AuthenticatedEntity::System)
        .await
        .unwrap();

    // Whoever `current` holds is the authenticated caller of every request.
    let current = Arc::new(Mutex::new(session(&stale)));
    let caller = current.clone();
    let app = Router::new()
        .route("/api/v1/hosts", get(|| async { "ok" }))
        .route("/api/v1/organizations", get(|| async { "ok" }))
        .route("/api/auth/me", post(|| async { "ok" }))
        .route("/api/billing/checkout", post(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            terms_gate_middleware,
        ))
        .layer(middleware::from_fn(
            move |mut request: Request<Body>, next: Next| {
                let entity = caller.lock().unwrap().clone();
                async move {
                    request.extensions_mut().insert(entity);
                    next.run(request).await
                }
            },
        ))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    // Refused everywhere except what the gate itself needs.
    let (code, body) = status(addr, reqwest::Method::GET, "/api/v1/hosts").await;
    assert_eq!(code, 403);
    assert!(body.contains("auth_terms_acceptance_required"));
    assert_eq!(
        status(addr, reqwest::Method::POST, "/api/billing/checkout")
            .await
            .0,
        403
    );
    assert_eq!(
        status(addr, reqwest::Method::POST, "/api/auth/me").await.0,
        200
    );
    assert_eq!(
        status(addr, reqwest::Method::GET, "/api/v1/organizations")
            .await
            .0,
        200
    );

    // An API key acting for the same user is never gated.
    *current.lock().unwrap() = AuthenticatedEntity::ApiKey {
        api_key_id: Uuid::new_v4(),
        user_id: stale.id,
        organization_id: org.id,
        permissions: UserOrgPermissions::Owner,
        site_ids: vec![],
    };
    assert_eq!(
        status(addr, reqwest::Method::GET, "/api/v1/hosts").await.0,
        200
    );

    // A demo org is never gated, even with no acceptance on record.
    let mut demo_user = user(&demo_org.id);
    demo_user.base.terms_accepted_at = None;
    *current.lock().unwrap() = session(&demo_user);
    assert_eq!(
        status(addr, reqwest::Method::GET, "/api/v1/hosts").await.0,
        200
    );

    // Accepting records now, which the gate lets through.
    let accepted = state
        .services
        .user_service
        .accept_terms(stale)
        .await
        .unwrap();
    let accepted_at = accepted.base.terms_accepted_at.unwrap();
    assert!(Utc::now() - accepted_at < Duration::minutes(1));
    assert!(!acceptance_required(
        &state.config,
        org.base.plan.as_ref(),
        accepted.base.terms_accepted_at
    ));
    *current.lock().unwrap() = session(&accepted);
    assert_eq!(
        status(addr, reqwest::Method::GET, "/api/v1/hosts").await.0,
        200
    );
}
