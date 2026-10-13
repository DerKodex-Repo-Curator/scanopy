use crate::daemon::discovery::types::warnings::RunWarningSummary;
use crate::server::openapi::tags as api_tags;
use crate::server::{
    auth::middleware::permissions::{Authorized, Viewer},
    config::AppState,
    daemons::r#impl::{api::DaemonResponse, base::Daemon, version::DaemonVersionPolicy},
    discovery::r#impl::{base::Discovery, types::RunType},
    shared::{
        services::traits::CrudService,
        storage::filter::StorableFilter,
        types::api::{ApiError, ApiResponse, ApiResult},
    },
    sites::r#impl::Site,
};

use super::types::{
    DashboardSummary, PlanUsage, RecentDiscovery, SiteSummary, UncoveredIntegration,
};
use crate::server::credentials::service::FoundIntegration;

use axum::{extract::State, response::Json};
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn create_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_dashboard_summary))
}

/// Get dashboard summary
///
/// Returns aggregated dashboard data including site metrics, daemon health,
/// recent discoveries, and plan usage.
#[utoipa::path(
    get,
    path = "/summary",
    tags = [api_tags::DASHBOARD, api_tags::INTERNAL],
    responses(
        (status = 200, description = "Dashboard summary", body = ApiResponse<DashboardSummary>),
    ),
    security(("user_api_key" = []), ("session" = []))
)]
async fn get_dashboard_summary(
    State(state): State<Arc<AppState>>,
    auth: Authorized<Viewer>,
) -> ApiResult<Json<ApiResponse<DashboardSummary>>> {
    let site_ids = auth.site_ids();
    let organization_id = auth
        .organization_id()
        .ok_or_else(|| ApiError::forbidden("Organization context required"))?;

    // Fetch sites the user has access to (filter by id, not site_id)
    let sites_filter = StorableFilter::<Site>::new_from_entity_ids(&site_ids);
    let sites_result = state
        .services
        .site_service
        .get_paginated(sites_filter)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;

    // Build per-site summaries with counts. `count_for_sites` narrows
    // SCD2 entities to live rows itself, so snapshot closed-copies aren't
    // counted (daemons are non-SCD2 → plain count).
    let mut site_summaries = Vec::new();
    for site in &sites_result.items {
        let ids = [site.id];
        let map_err = |e: anyhow::Error| ApiError::internal_error(&e.to_string());
        site_summaries.push(SiteSummary {
            id: site.id,
            name: site.base.name.clone(),
            host_count: state
                .services
                .host_service
                .count_for_sites(&ids)
                .await
                .map_err(map_err)?,
            service_count: state
                .services
                .service_service
                .count_for_sites(&ids)
                .await
                .map_err(map_err)?,
            subnet_count: state
                .services
                .subnet_service
                .count_for_sites(&ids)
                .await
                .map_err(map_err)?,
            daemon_count: state
                .services
                .daemon_service
                .count_for_sites(&ids)
                .await
                .map_err(map_err)?,
        });
    }

    // Fetch all daemons with version status
    let daemons_filter = StorableFilter::<Daemon>::new_from_site_ids(&site_ids);
    let daemons_result = state
        .services
        .daemon_service
        .get_paginated(daemons_filter)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;

    let daemon_ids: Vec<_> = daemons_result.items.iter().map(|d| d.id).collect();
    let subnet_ids_map = state
        .services
        .daemon_service
        .get_interfaced_subnet_ids_batch(&daemon_ids)
        .await;

    let policy = DaemonVersionPolicy::default();
    let daemon_responses: Vec<DaemonResponse> = daemons_result
        .items
        .into_iter()
        .map(|d| {
            let version_status = policy.evaluate(d.base.version.as_ref());
            let interfaced_subnet_ids = subnet_ids_map.get(&d.id).cloned().unwrap_or_default();
            DaemonResponse {
                id: d.id,
                created_at: d.created_at,
                updated_at: d.updated_at,
                base: d.base,
                version_status,
                interfaced_subnet_ids,
            }
        })
        .collect();

    // Fetch recent historical discoveries (last 5)
    let discovery_filter = StorableFilter::<Discovery>::new_from_site_ids(&site_ids)
        .historical_discovery()
        .limit(5);
    let discovery_result = state
        .services
        .discovery_service
        .get_paginated_ordered(discovery_filter, "created_at DESC")
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;

    // Plan usage
    let plan = state
        .services
        .organization_service
        .get_by_id(&organization_id)
        .await?
        .and_then(|o| o.base.plan)
        .unwrap_or_else(crate::server::billing::plans::get_free_plan);
    let (host_limit, site_limit, seat_limit) =
        (plan.host_limit(), plan.site_limit(), plan.seat_limit());

    // Total host count across all sites (live only) and org seat count.
    let host_count = state
        .services
        .host_service
        .count_for_sites(&site_ids)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;
    let seat_count = state
        .services
        .user_service
        .count_for_org(&organization_id)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;

    let recent_discoveries = discovery_result
        .items
        .into_iter()
        .map(|discovery| {
            let warning_summary = match &discovery.base.run_type {
                RunType::Historical { results } => {
                    RunWarningSummary::from(results.warnings.as_slice())
                }
                _ => RunWarningSummary::default(),
            };
            RecentDiscovery {
                discovery,
                warning_summary,
            }
        })
        .collect();

    // What the latest full scans found with no credential, re-checked against today's
    // assignments so a credential saved since clears its nudge without waiting for a scan.
    let (found, tried_at) = state
        .services
        .discovery_service
        .latest_integrations_without_credentials(&site_ids)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;
    let uncovered = state
        .services
        .credential_service
        .uncovered_integrations(found, &[], None, &tried_at)
        .await
        .map_err(|e| ApiError::internal_error(&e.to_string()))?;
    let uncovered_integrations = group_by_integration(uncovered);

    let plan_usage = PlanUsage {
        host_limit,
        host_count,
        site_limit,
        site_count: sites_result.total_count,
        seat_limit,
        seat_count,
    };

    Ok(Json(ApiResponse::success(DashboardSummary {
        sites: site_summaries,
        daemons: daemon_responses,
        recent_discoveries,
        plan_usage,
        uncovered_integrations,
    })))
}

/// One entry per integration, naming every host it was found on, most hosts first.
fn group_by_integration(found: Vec<FoundIntegration>) -> Vec<UncoveredIntegration> {
    let mut grouped: Vec<UncoveredIntegration> = Vec::new();
    for f in found {
        match grouped.iter_mut().find(|g| g.integration == f.integration) {
            Some(g) => {
                g.host_ids.push(f.host_id);
                if !g.site_ids.contains(&f.site_id) {
                    g.site_ids.push(f.site_id);
                }
            }
            None => grouped.push(UncoveredIntegration {
                integration: f.integration,
                host_ids: vec![f.host_id],
                site_ids: vec![f.site_id],
            }),
        }
    }
    grouped.sort_by(|a, b| b.host_ids.len().cmp(&a.host_ids.len()));
    grouped
}
