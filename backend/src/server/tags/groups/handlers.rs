use crate::server::auth::middleware::permissions::{Admin, Authorized, Member, Viewer};
use crate::server::shared::handlers::query::NoFilterQuery;
use crate::server::shared::handlers::traits::{CrudHandlers, create_handler};
use crate::server::shared::services::traits::CrudService;
use crate::server::shared::storage::traits::Entity;
use crate::server::shared::types::api::ApiJson;
use crate::server::shared::types::api::{ApiError, ApiErrorResponse, PaginatedApiResponse};
use crate::server::tags::groups::{base::TagGroup, service::TagGroupService};
use crate::server::tags::r#impl::base::Tag;
use crate::server::{
    config::AppState,
    shared::types::api::{ApiResponse, ApiResult},
};
use axum::extract::Path;
use axum::{extract::State, response::Json};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

impl CrudHandlers for TagGroup {
    type Service = TagGroupService;
    type FilterQuery = NoFilterQuery;
    type OrderField = crate::server::shared::handlers::ordering::NoOrderField;

    fn get_service(state: &AppState) -> &Self::Service {
        &state.services.tag_group_service
    }
}

mod generated {
    use super::*;
    crate::crud_get_by_id_handler!(TagGroup);
    crate::crud_update_handler!(TagGroup);
    crate::crud_delete_handler!(TagGroup);
    crate::crud_bulk_delete_handler!(TagGroup);
}

pub fn create_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_all_tag_groups, create_tag_group))
        .routes(routes!(
            generated::get_by_id,
            generated::update,
            generated::delete
        ))
        .routes(routes!(generated::bulk_delete))
        .routes(routes!(set_tag_order))
}

/// List all tag groups
///
/// Returns every tag group in your organization: the system groups (Application, Status) first,
/// then the groups your organization named, by name.
#[utoipa::path(
    get,
    path = "",
    tag = TagGroup::ENTITY_NAME_PLURAL,
    params(NoFilterQuery),
    responses(
        (status = 200, description = "List of tag groups", body = inline(PaginatedApiResponse<TagGroup>)),
    ),
    security(("user_api_key" = []), ("session" = []))
)]
async fn get_all_tag_groups(
    State(state): State<Arc<AppState>>,
    auth: Authorized<Viewer>,
) -> ApiResult<Json<PaginatedApiResponse<TagGroup>>> {
    let organization_id = auth
        .organization_id()
        .ok_or_else(ApiError::organization_required)?;
    let groups = state
        .services
        .tag_group_service
        .get_for_org(&organization_id)
        .await?;
    let total = groups.len() as u64;
    Ok(Json(PaginatedApiResponse::success(groups, total, 0, 0)))
}

/// Create a tag group
///
/// Creates a group your organization names. Group names are unique within the organization, and
/// "Application" and "Status" are reserved for the system groups.
#[utoipa::path(
    post,
    path = "",
    tag = TagGroup::ENTITY_NAME_PLURAL,
    request_body = TagGroup,
    responses(
        (status = 200, description = "Tag group created", body = ApiResponse<TagGroup>),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
    ),
    security(("user_api_key" = []), ("session" = []))
)]
async fn create_tag_group(
    State(state): State<Arc<AppState>>,
    auth: Authorized<Admin>,
    ApiJson(group): ApiJson<TagGroup>,
) -> ApiResult<Json<ApiResponse<TagGroup>>> {
    create_handler::<TagGroup>(State(state), auth.into_permission::<Member>(), ApiJson(group)).await
}

/// Request body for setting a tag group's tag order
#[derive(Debug, Deserialize, ToSchema)]
pub struct TagOrderRequest {
    /// Every tag in the group, each once, in the order to list them.
    pub tag_ids: Vec<Uuid>,
}

/// Set the order of a tag group's tags
///
/// Tables, pickers and the topology list a group's tags in this order.
#[utoipa::path(
    put,
    path = "/{id}/tag-order",
    tag = TagGroup::ENTITY_NAME_PLURAL,
    params(("id" = Uuid, Path, description = "Tag group ID")),
    request_body = TagOrderRequest,
    responses(
        (status = 200, description = "The group's tags, in their new order", body = ApiResponse<Vec<Tag>>),
        (status = 400, description = "The list doesn't name every tag in the group exactly once", body = ApiErrorResponse),
        (status = 404, description = "Tag group not found", body = ApiErrorResponse),
    ),
    security(("user_api_key" = []), ("session" = []))
)]
async fn set_tag_order(
    State(state): State<Arc<AppState>>,
    auth: Authorized<Member>,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<TagOrderRequest>,
) -> ApiResult<Json<ApiResponse<Vec<Tag>>>> {
    let organization_id = auth
        .organization_id()
        .ok_or_else(ApiError::organization_required)?;
    let service = &state.services.tag_group_service;
    let group = service
        .get_by_id(&id)
        .await?
        .filter(|g| g.base.organization_id == organization_id)
        .ok_or_else(|| ApiError::not_found("Tag group not found".to_string()))?;

    let tags = service.reorder_tags(&group, &request.tag_ids).await?;
    Ok(Json(ApiResponse::success(tags)))
}
