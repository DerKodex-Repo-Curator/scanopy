//! Public email endpoints: one-click unsubscribe (RFC 8058) and SES feedback
//! from Amazon SNS. Neither takes a session. Both touch only users, so they
//! work whether or not this server has email configured.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Redirect;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::feedback::FeedbackError;
use super::messages::{EmailCategory, links};
use crate::server::config::AppState;
use crate::server::openapi::tags as api_tags;
use crate::server::shared::types::api::{
    ApiError, ApiErrorResponse, ApiResponse, ApiResult, EmptyApiResponse,
};
use crate::server::shared::types::metadata::HasId;
use strum::IntoEnumIterator;

/// Where the router is mounted. The rate limiter and license guard let the
/// routes below through by these paths: a mail client's one-click POST and an
/// SNS delivery have no session to rate-limit by, and an unsubscribe has to
/// work while a license is locked.
pub const EMAIL_API_PREFIX: &str = "/api/email";
pub const UNSUBSCRIBE_PATH_PREFIX: &str = "/api/email/unsubscribe/";
pub const SNS_FEEDBACK_PATH: &str = "/api/email/feedback/sns";

pub fn create_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(open_unsubscribe_link, unsubscribe))
        .routes(routes!(receive_sns_feedback))
}

/// One-click unsubscribe
///
/// Turns off one pausable email category for the user the token belongs to.
/// Mail clients POST here from the `List-Unsubscribe` header. Answers 200
/// whether or not the token matched, so the endpoint does not reveal which
/// tokens exist. The request body is ignored.
#[utoipa::path(
    post,
    path = "/unsubscribe/{token}/{category}",
    tags = [api_tags::INTERNAL],
    params(
        ("token" = String, Path, description = "The recipient's unsubscribe token"),
        ("category" = String, Path, description = "Pausable email category, e.g. discovery_digest"),
    ),
    responses(
        (status = 200, description = "Request accepted", body = EmptyApiResponse),
        (status = 400, description = "Unknown email category", body = ApiErrorResponse),
    )
)]
pub(crate) async fn unsubscribe(
    State(state): State<Arc<AppState>>,
    Path((token, category)): Path<(String, String)>,
) -> ApiResult<Json<ApiResponse<()>>> {
    let category = EmailCategory::iter()
        .find(|c| c.pausable() && c.id() == category)
        .ok_or_else(|| ApiError::bad_request("Unknown email category"))?;
    let Ok(token) = Uuid::parse_str(&token) else {
        return Ok(Json(ApiResponse::success(())));
    };

    let user_service = &state.services.user_service;
    if let Some(user) = user_service.get_by_unsubscribe_token(&token).await? {
        let user_id = user.id;
        let changed = user_service
            .update_email_settings(user, |settings| {
                settings.set(category, false).unwrap_or(false)
            })
            .await?;
        tracing::info!(
            %user_id,
            category = category.id(),
            changed,
            "One-click unsubscribe"
        );
    }
    Ok(Json(ApiResponse::success(())))
}

/// Open an unsubscribe link in a browser
///
/// A mail client that does not support one-click opens the `List-Unsubscribe`
/// URL with a GET. That changes nothing: it redirects to Settings → Email,
/// where the user chooses what to turn off.
#[utoipa::path(
    get,
    path = "/unsubscribe/{token}/{category}",
    tags = [api_tags::INTERNAL],
    params(
        ("token" = String, Path, description = "The recipient's unsubscribe token"),
        ("category" = String, Path, description = "Pausable email category, e.g. discovery_digest"),
    ),
    responses(
        (status = 303, description = "Redirect to the email preferences page"),
    )
)]
async fn open_unsubscribe_link(
    State(state): State<Arc<AppState>>,
    Path((_token, _category)): Path<(String, String)>,
) -> Redirect {
    let url = links::SETTINGS_EMAIL
        .replace("{base_url}", state.config.public_url.trim_end_matches('/'))
        .replace(
            "{utm}",
            "utm_source=email&utm_campaign=list_unsubscribe&utm_medium=email",
        );
    Redirect::to(&url)
}

/// Receive SES feedback from Amazon SNS
///
/// The HTTPS subscription endpoint for the SNS topic named by
/// `SCANOPY_EMAIL_FEEDBACK_SNS_TOPIC_ARN`. Confirms the subscription, and
/// applies SES bounce and complaint notifications. Answers 404 when no topic
/// is configured.
#[utoipa::path(
    post,
    path = "/feedback/sns",
    tags = [api_tags::INTERNAL],
    request_body(content = String, content_type = "text/plain", description = "SNS message JSON"),
    responses(
        (status = 200, description = "Message processed", body = EmptyApiResponse),
        (status = 400, description = "Not an authentic message for the configured topic", body = ApiErrorResponse),
        (status = 404, description = "SES feedback is not configured", body = ApiErrorResponse),
    )
)]
async fn receive_sns_feedback(
    State(state): State<Arc<AppState>>,
    body: String,
) -> ApiResult<Json<ApiResponse<()>>> {
    let Some(feedback_service) = &state.services.email_feedback_service else {
        return Err(ApiError::not_found("Not found".to_string()));
    };
    match feedback_service.handle(&body).await {
        Ok(()) => Ok(Json(ApiResponse::success(()))),
        Err(FeedbackError::Rejected(reason)) => {
            tracing::warn!(reason = reason.as_str(), "Rejected an SNS message");
            Err(ApiError::bad_request("Invalid message"))
        }
        Err(FeedbackError::Unavailable(e)) => {
            tracing::error!(error = %e, "Could not process an SNS message");
            Err(ApiError::internal_error("Could not process the message"))
        }
    }
}
