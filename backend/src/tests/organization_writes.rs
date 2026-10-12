//! Org read-modify-writes go through `OrganizationService::lock_for_update`, so two writers of
//! different columns that overlap both land: the second reads the row after the first saves it.

use super::{organization, test_services};
use crate::server::{
    auth::middleware::auth::AuthenticatedEntity,
    organizations::r#impl::base::LimitNotificationLevel, shared::services::traits::CrudService,
};

#[tokio::test]
async fn overlapping_writers_of_different_columns_both_persist() {
    let (_storage, services, _container) = test_services().await;
    let org = services
        .organization_service
        .create(organization(), AuthenticatedEntity::System)
        .await
        .unwrap();
    let organization_service = services.organization_service.clone();

    let mut renaming = organization_service
        .lock_for_update(org.id)
        .await
        .unwrap()
        .unwrap();

    // Starts while the rename holds the row, the way a background ratchet write overlaps a
    // user's request.
    let ratchet = tokio::spawn({
        let organization_service = organization_service.clone();
        async move {
            organization_service
                .modify(org.id, AuthenticatedEntity::System, |org| {
                    org.base.notifications.hosts = LimitNotificationLevel::Reached;
                })
                .await
                .unwrap()
        }
    });
    tokio::task::yield_now().await;

    renaming.org.base.name = "Renamed".to_string();
    renaming.save(AuthenticatedEntity::System).await.unwrap();
    ratchet.await.unwrap().expect("org exists");

    let stored = organization_service
        .get_by_id(&org.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.base.name, "Renamed");
    assert_eq!(
        stored.base.notifications.hosts,
        LimitNotificationLevel::Reached
    );
}
