//! A finished discovery session stamps the discovery FKs on what it scanned before
//! `update_session` returns, with no event-bus subscriber in between.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::daemon::discovery::types::base::DiscoveryPhase;
use crate::server::auth::middleware::auth::AuthenticatedEntity;
use crate::server::daemons::r#impl::api::{DiscoveryUpdatePayload, ScannedEntityIds};
use crate::server::discovery::r#impl::types::DiscoveryType;
use crate::server::shared::services::traits::CrudService;
use crate::server::shared::storage::traits::Storage;

use super::{daemon, host, organization, site, test_services, user};

#[tokio::test]
async fn finished_session_stamps_scanned_host_with_its_historical_discovery() {
    let (storage, services, _container) = test_services().await;

    let org = organization();
    storage.organizations.create(&org).await.unwrap();
    let site_row = services
        .site_service
        .create(site(&org.id), AuthenticatedEntity::System)
        .await
        .unwrap();

    let scanned_host = services
        .host_service
        .create(host(&site_row.id), AuthenticatedEntity::System)
        .await
        .unwrap();
    let untouched_host = services
        .host_service
        .create(host(&site_row.id), AuthenticatedEntity::System)
        .await
        .unwrap();

    let owner = user(&org.id);
    storage.users.create(&owner).await.unwrap();
    let mut d = daemon(&site_row.id, &scanned_host.id);
    d.base.user_id = owner.id;
    storage.daemons.create(&d).await.unwrap();

    let mut update = DiscoveryUpdatePayload::new(
        Uuid::new_v4(),
        d.id,
        site_row.id,
        DiscoveryType::SelfReport {
            host_id: scanned_host.id,
        },
        None,
    );
    update.phase = DiscoveryPhase::Complete;
    update.progress = 100;
    update.started_at = Some(Utc::now() - Duration::minutes(1));
    update.finished_at = Some(Utc::now());
    update.scanned = Some(ScannedEntityIds {
        host_ids: vec![scanned_host.id],
        ..Default::default()
    });

    services
        .discovery_service
        .update_session(update)
        .await
        .unwrap();

    let stamped = storage
        .hosts
        .get_by_id(&scanned_host.id)
        .await
        .unwrap()
        .unwrap();
    let discovery_id = stamped
        .last_discovery_id
        .expect("the scanned host points at the session's discovery");
    assert_eq!(stamped.first_discovery_id, Some(discovery_id));

    let historical = storage
        .discovery
        .get_by_id(&discovery_id)
        .await
        .unwrap()
        .expect("the FK names a persisted discovery row");
    assert!(historical.base.run_type.historical_results().is_some());

    let untouched = storage
        .hosts
        .get_by_id(&untouched_host.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(untouched.last_discovery_id, None);
}
