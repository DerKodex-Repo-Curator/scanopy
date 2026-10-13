/// Every filter the services list documents narrows the rows, through the one filter the list and
/// the CSV export build. The export once applied only the access filter, so `search`, tags and
/// every field filter were accepted and ignored there.
#[tokio::test]
async fn every_documented_service_filter_narrows_the_rows() {
    use crate::server::services::handlers::ServiceFilterQuery;
    use crate::server::shared::handlers::query::narrowing_check::assert_every_param_narrows;
    use uuid::Uuid;

    let (site, other_site, org) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let id = Uuid::new_v4().to_string();
    let site_param = site.to_string();
    assert_every_param_narrows(
        |query: &ServiceFilterQuery| {
            query
                .narrow(&[site, other_site], org)
                .expect("filter builds")
        },
        &[
            ("site_ids", site_param.as_str()),
            ("host_ids", id.as_str()),
            ("ids", id.as_str()),
            ("service_definitions", "Docker"),
            ("virtualization_service_ids", id.as_str()),
            ("virtualization_service_names", "Docker"),
            ("include_uncontainerized", "true"),
            ("sources", "DiscoveryWithMatch"),
            ("match_confidences", "High"),
            ("tag_ids", id.as_str()),
            ("tag_sets", id.as_str()),
            ("search", "nas"),
            ("ports", "53"),
            ("exclude_categories", "OpenPorts"),
            ("at", "2026-01-01T00:00:00Z"),
        ],
        &[
            // Paging and ordering.
            "limit",
            "offset",
            "group_by",
            "order_by",
            "order_direction",
            "group_by_tag_group",
            "order_by_tag_group",
            // Applied after `narrow`, from each site's cutoff.
            "stale",
        ],
    )
    .await;
}
