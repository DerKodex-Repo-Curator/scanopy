use crate::server::{
    config::AppState,
    services::{handlers::ServiceFilterQuery, r#impl::base::Service, service::ServiceService},
    shared::{
        handlers::traits::CrudHandlers, storage::filter::StorableFilter, types::api::ApiResult,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
impl CrudHandlers for Service {
    type Service = ServiceService;
    type FilterQuery = ServiceFilterQuery;
    type OrderField = crate::server::services::handlers::ServiceOrderField;

    fn get_service(state: &AppState) -> &Self::Service {
        &state.services.service_service
    }

    fn search_order() -> Option<Self::OrderField> {
        Some(Self::OrderField::Name)
    }

    /// Everything the Services list narrows by, shared by the list and the CSV export so an
    /// export returns exactly the rows the list showed for the same query.
    async fn list_filter(
        state: &AppState,
        query: &ServiceFilterQuery,
        site_ids: &[Uuid],
        organization_id: Uuid,
    ) -> ApiResult<StorableFilter<Service>> {
        let filter = query.narrow(site_ids, organization_id)?;

        // Staleness is per-site, so resolve each accessible site's cutoff and
        // let the filter compare every row against its own.
        Ok(match query.stale {
            Some(stale) => {
                let cutoffs = state.services.site_service.stale_cutoffs(site_ids).await?;
                filter.stale_by_site(&cutoffs, stale)
            }
            None => filter,
        })
    }
}
