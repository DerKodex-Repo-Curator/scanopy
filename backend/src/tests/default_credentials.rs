//! Seeded default credentials through the services: an organization starts with one per seeded
//! type, a new site gets the site-wide ones, and a default can be renamed and unassigned but not
//! deleted, changed or copied.

use secrecy::SecretString;
use strum::IntoEnumIterator;
use uuid::Uuid;

use super::{organization, site, test_services};
use crate::server::{
    auth::middleware::auth::AuthenticatedEntity,
    credentials::r#impl::{
        base::{Credential, CredentialBase},
        types::{
            CredentialType, CredentialTypeDiscriminants, REDACTED_SECRET_SENTINEL, SecretValue,
        },
    },
    shared::{
        services::{factory::ServiceFactory, traits::CrudService},
        storage::filter::StorableFilter,
        types::{api::ApiError, error_codes::ErrorCode},
    },
};

fn community(value: &str) -> CredentialType {
    CredentialType::SnmpV2c {
        community: SecretValue::Inline {
            value: SecretString::from(value.to_string()),
        },
    }
}

fn credential(organization_id: Uuid, credential_type: CredentialType) -> Credential {
    Credential::new(CredentialBase {
        organization_id,
        credential_type,
        ..Default::default()
    })
}

async fn org_credentials(services: &ServiceFactory, organization_id: Uuid) -> Vec<Credential> {
    services
        .credential_service
        .get_all(StorableFilter::<Credential>::new_from_org_id(
            &organization_id,
        ))
        .await
        .unwrap()
}

async fn snmp_default(services: &ServiceFactory, organization_id: Uuid) -> Credential {
    services
        .credential_service
        .find_default(organization_id, CredentialTypeDiscriminants::SnmpV2c)
        .await
        .unwrap()
        .expect("the organization has an SNMP default")
}

/// The error code a refused service call carried.
fn error_code(result: Result<impl std::fmt::Debug, anyhow::Error>) -> ErrorCode {
    let err = result.expect_err("the service accepted it");
    err.downcast_ref::<ApiError>()
        .and_then(|e| e.error_code.clone())
        .unwrap_or_else(|| panic!("not a coded error: {err}"))
}

async fn new_org(services: &ServiceFactory) -> Uuid {
    services
        .organization_service
        .create(organization(), AuthenticatedEntity::System)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn a_new_organization_holds_each_seeded_default_once() {
    let (_storage, services, _container) = test_services().await;
    let organization_id = new_org(&services).await;

    let seeded_types: Vec<CredentialTypeDiscriminants> = CredentialTypeDiscriminants::iter()
        .filter(|d| d.seeded_default().is_some())
        .collect();
    let defaults_after_create = org_credentials(&services, organization_id).await;
    assert_eq!(defaults_after_create.len(), seeded_types.len());
    assert!(defaults_after_create.iter().all(|c| c.is_default));

    let created_again = services
        .credential_service
        .seed_defaults(organization_id)
        .await
        .unwrap();
    assert!(created_again.is_empty());
    assert_eq!(
        org_credentials(&services, organization_id).await.len(),
        seeded_types.len()
    );
}

#[tokio::test]
async fn a_new_site_without_named_credentials_gets_the_site_wide_defaults() {
    let (_storage, services, _container) = test_services().await;
    let organization_id = new_org(&services).await;

    let created = services
        .site_service
        .create(site(&organization_id), AuthenticatedEntity::System)
        .await
        .unwrap();

    let assigned = services
        .credential_service
        .get_credential_ids_for_site(&created.id)
        .await
        .unwrap();
    assert_eq!(
        assigned,
        vec![snmp_default(&services, organization_id).await.id]
    );

    // A request that names its credentials, even none, is taken as it is.
    let mut without = site(&organization_id);
    without.base.credential_ids = Some(vec![]);
    let created = services
        .site_service
        .create(without, AuthenticatedEntity::System)
        .await
        .unwrap();
    let assigned = services
        .credential_service
        .get_credential_ids_for_site(&created.id)
        .await
        .unwrap();
    assert!(assigned.is_empty());
}

#[tokio::test]
async fn a_second_copy_of_a_default_is_refused_on_create_and_update() {
    let (_storage, services, _container) = test_services().await;
    let organization_id = new_org(&services).await;

    let copy = services
        .credential_service
        .create(
            credential(organization_id, community("public")),
            AuthenticatedEntity::System,
        )
        .await;
    assert!(matches!(
        error_code(copy),
        ErrorCode::CredentialDefaultExists { .. }
    ));

    let mut ordinary = services
        .credential_service
        .create(
            credential(organization_id, community("private")),
            AuthenticatedEntity::System,
        )
        .await
        .unwrap();
    assert!(!ordinary.is_default);
    ordinary.base.credential_type = community("public");
    let turned_into_copy = services
        .credential_service
        .update(&mut ordinary, AuthenticatedEntity::System)
        .await;
    assert!(matches!(
        error_code(turned_into_copy),
        ErrorCode::CredentialDefaultExists { .. }
    ));
}

#[tokio::test]
async fn a_default_can_be_renamed_and_unassigned_but_not_changed_or_deleted() {
    let (_storage, services, _container) = test_services().await;
    let organization_id = new_org(&services).await;
    let created_site = services
        .site_service
        .create(site(&organization_id), AuthenticatedEntity::System)
        .await
        .unwrap();
    let default = snmp_default(&services, organization_id).await;

    // The UI sends the community back redacted, as it read it.
    let mut renamed = default.clone();
    renamed.base.name = "Factory community".to_string();
    renamed.base.credential_type = community(REDACTED_SECRET_SENTINEL);
    let renamed = services
        .credential_service
        .update(&mut renamed, AuthenticatedEntity::System)
        .await
        .unwrap();
    assert!(renamed.is_default);

    let mut changed = default.clone();
    changed.base.credential_type = community("private");
    let changed = services
        .credential_service
        .update(&mut changed, AuthenticatedEntity::System)
        .await;
    assert!(matches!(
        error_code(changed),
        ErrorCode::CredentialDefaultSettingsLocked { .. }
    ));

    let deleted = services
        .credential_service
        .delete(&default.id, AuthenticatedEntity::System)
        .await;
    assert!(matches!(
        error_code(deleted),
        ErrorCode::CredentialDefaultDeleteForbidden { .. }
    ));

    // A batch naming a default deletes nothing, not even the ordinary credential in it.
    let ordinary = services
        .credential_service
        .create(
            credential(organization_id, community("private")),
            AuthenticatedEntity::System,
        )
        .await
        .unwrap();
    let batch = services
        .credential_service
        .delete_many(&[ordinary.id, default.id], AuthenticatedEntity::System)
        .await;
    assert!(matches!(
        error_code(batch),
        ErrorCode::CredentialDefaultDeleteForbidden { .. }
    ));
    assert!(
        services
            .credential_service
            .get_by_id(&ordinary.id)
            .await
            .unwrap()
            .is_some()
    );

    services
        .credential_service
        .set_site_credentials(&created_site.id, &[])
        .await
        .unwrap();
    assert!(
        services
            .credential_service
            .get_credential_ids_for_site(&created_site.id)
            .await
            .unwrap()
            .is_empty()
    );
}
