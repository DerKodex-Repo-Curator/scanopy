//! `OrganizationService::record_milestone`: the caller that adds a milestone is the one that
//! publishes its event, however many requests reach it together.

use super::{organization, test_services};
use crate::server::shared::{
    events::types::OnboardingOperationDiscriminants, services::traits::CrudService,
    storage::traits::Storage,
};

#[tokio::test]
async fn concurrent_records_of_one_milestone_add_it_once() {
    let (storage, services, _container) = test_services().await;
    let org = organization();
    storage.organizations.create(&org).await.unwrap();

    let step = OnboardingOperationDiscriminants::FirstTagCreated;
    let organizations = &services.organization_service;
    let (a, b) = tokio::join!(
        organizations.record_milestone(org.id, step),
        organizations.record_milestone(org.id, step),
    );
    let added = [a.unwrap(), b.unwrap()];
    assert_eq!(added.iter().filter(|added| **added).count(), 1);

    let stored = organizations.get_by_id(&org.id).await.unwrap().unwrap();
    assert_eq!(
        stored
            .base
            .onboarding
            .iter()
            .filter(|recorded| **recorded == step)
            .count(),
        1
    );
}
