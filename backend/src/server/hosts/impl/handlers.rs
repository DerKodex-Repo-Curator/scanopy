use crate::server::{
    config::AppState,
    hosts::{handlers::HostFilterQuery, r#impl::base::Host, service::HostService},
    shared::{
        handlers::traits::CrudHandlers, storage::filter::StorableFilter, types::api::ApiResult,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
impl CrudHandlers for Host {
    type Service = HostService;
    type FilterQuery = HostFilterQuery;
    type OrderField = crate::server::hosts::handlers::HostOrderField;

    fn get_service(state: &AppState) -> &Self::Service {
        &state.services.host_service
    }

    fn search_order() -> Option<Self::OrderField> {
        Some(Self::OrderField::Name)
    }

    /// Everything the Hosts list narrows by, shared by the list and both exports so an export
    /// returns exactly the rows the list showed for the same query.
    async fn list_filter(
        state: &AppState,
        query: &HostFilterQuery,
        site_ids: &[Uuid],
        organization_id: Uuid,
    ) -> ApiResult<StorableFilter<Host>> {
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
