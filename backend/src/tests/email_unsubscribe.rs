//! One-click unsubscribe links on outgoing email, the endpoint they point at,
//! and SES complaints turning a user's pausable email off.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use email_address::EmailAddress;
use strum::IntoEnumIterator;

use super::self_hosted_license_emails::{create_owner, sent};
use super::self_hosted_licensing::{create_org, test_state_with_email_dir};
use crate::server::billing::plans::get_enterprise_plan;
use crate::server::config::AppState;
use crate::server::email::feedback::{EmailFeedbackService, SesFeedback};
use crate::server::email::handlers::unsubscribe;
use crate::server::email::messages::EmailCategory;
use crate::server::shared::types::metadata::HasId;
use crate::server::users::r#impl::base::User;

fn owner_email() -> EmailAddress {
    EmailAddress::new_unchecked("owner@example.test")
}

async fn owner(state: &AppState) -> User {
    state
        .services
        .user_service
        .get_by_email(&owner_email())
        .await
        .unwrap()
        .unwrap()
}

/// The `List-Unsubscribe` URLs the logging transport recorded.
fn unsubscribe_urls(sent: &str) -> Vec<String> {
    sent.lines()
        .filter_map(|line| line.strip_prefix("List-Unsubscribe: <"))
        .filter_map(|rest| rest.strip_suffix('>'))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn only_pausable_email_to_a_known_user_carries_an_unsubscribe_link() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _container) = test_state_with_email_dir(Some(dir.path().to_path_buf())).await;
    let org = create_org(&state, get_enterprise_plan(), None).await;
    create_owner(&state, org.id).await;
    let email_service = state.services.email_service.clone().unwrap();

    email_service
        .send_password_changed_email(owner_email(), "now")
        .await
        .unwrap();
    email_service
        .send_discovery_guide_email(
            EmailAddress::new_unchecked("nobody@example.test"),
            "daemon",
            "site",
        )
        .await
        .unwrap();
    assert!(
        unsubscribe_urls(&sent(dir.path())).is_empty(),
        "a required email, and a pausable one to an address with no account, carry no link"
    );

    for _ in 0..2 {
        email_service
            .send_discovery_guide_email(owner_email(), "daemon", "site")
            .await
            .unwrap();
    }
    let token = owner(&state)
        .await
        .base
        .email_unsubscribe_token
        .expect("the first pausable send stores a token");
    let urls = unsubscribe_urls(&sent(dir.path()));
    assert_eq!(urls.len(), 2);
    assert!(
        urls.iter().all(|url| url.ends_with(&format!(
            "/api/email/unsubscribe/{token}/product_onboarding"
        ))),
        "both sends reuse the stored token: {urls:?}"
    );
}

#[tokio::test]
async fn one_click_unsubscribe_turns_off_only_the_named_category() {
    let (state, _container) = test_state_with_email_dir(None).await;
    let org = create_org(&state, get_enterprise_plan(), None).await;
    create_owner(&state, org.id).await;
    let token = state
        .services
        .user_service
        .ensure_unsubscribe_token(&owner(&state).await)
        .await
        .unwrap();

    let _ = unsubscribe(
        State(state.clone()),
        Path((token.to_string(), "daemon_alerts".to_string())),
    )
    .await
    .unwrap();

    let settings = owner(&state).await.base.email_settings;
    for category in EmailCategory::iter() {
        assert_eq!(
            settings.allows(category),
            category != EmailCategory::DaemonAlerts,
            "{}",
            category.id()
        );
    }

    // An unknown token answers the same as a known one.
    let _ = unsubscribe(
        State(state.clone()),
        Path((
            uuid::Uuid::new_v4().to_string(),
            "daemon_alerts".to_string(),
        )),
    )
    .await
    .unwrap();

    // A category that doesn't exist, and a required one, have nothing to turn off.
    for category in ["newsletter", "billing"] {
        let rejected = unsubscribe(
            State(state.clone()),
            Path((token.to_string(), category.to_string())),
        )
        .await
        .unwrap_err();
        assert_eq!(
            rejected.into_response().status(),
            StatusCode::BAD_REQUEST,
            "{category}"
        );
    }
}

#[tokio::test]
async fn a_complaint_turns_off_every_pausable_email_for_that_user() {
    let (state, _container) = test_state_with_email_dir(None).await;
    let org = create_org(&state, get_enterprise_plan(), None).await;
    create_owner(&state, org.id).await;
    let feedback = EmailFeedbackService::new(
        "arn:aws:sns:us-east-1:123456789012:ses-feedback".to_string(),
        state.services.user_service.clone(),
    )
    .unwrap();

    feedback
        .apply(SesFeedback::Complaint {
            recipients: vec![
                owner_email().to_string(),
                "nobody@example.test".to_string(),
                "not an address".to_string(),
            ],
        })
        .await
        .unwrap();

    let settings = owner(&state).await.base.email_settings;
    for category in EmailCategory::iter() {
        assert_eq!(settings.allows(category), !category.pausable());
    }
}
