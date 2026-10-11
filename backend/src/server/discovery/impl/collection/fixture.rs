//! The `collected-data` fixture: every category, how it is collected without a credential, and
//! which integrations collect it.

use serde::Serialize;
use strum::IntoEnumIterator;

use crate::server::{
    credentials::r#impl::types::CredentialTypeDiscriminants,
    shared::{
        attribution::AttributeSource,
        types::metadata::{HasId, TypeMetadataProvider},
    },
};

use super::{
    CollectedData,
    classify::{collecting_integrations, credential_free_methods},
};

/// One category in the `collected-data` fixture.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// The credential-free methods that collect this category on every host.
    pub methods: Vec<CollectedDataMethodEntry>,
    /// The integrations that collect this category with a credential.
    pub integrations: Vec<CollectedDataIntegrationEntry>,
}

/// A credential-free method, by its source and its label.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataMethodEntry {
    pub source: AttributeSource,
    /// [`AttributeSource::label`].
    pub name: String,
    /// [`AttributeSource::describe`].
    pub description: String,
}

/// An integration that collects a category, with the credential types that reach it.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataIntegrationEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// Whether the integration has a seeded default credential, so it runs on new sites or
    /// daemons until the user unassigns it.
    pub seeded_by_default: bool,
    pub credential_types: Vec<CredentialTypeEntry>,
}

/// A credential type, by id and display name.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialTypeEntry {
    pub id: &'static str,
    pub name: &'static str,
}

/// Every category in display order. Emitted as `collected-data.json` by `generate-fixtures`.
pub fn all_collected_data() -> Vec<CollectedDataEntry> {
    CollectedData::iter()
        .map(|category| CollectedDataEntry {
            id: category.id(),
            name: TypeMetadataProvider::name(&category),
            description: category.description(),
            methods: credential_free_methods(category)
                .into_iter()
                .map(|source| CollectedDataMethodEntry {
                    source,
                    name: source.label(),
                    description: source.describe(),
                })
                .collect(),
            integrations: collecting_integrations(category)
                .into_iter()
                .map(|integration| CollectedDataIntegrationEntry {
                    id: integration.id(),
                    name: TypeMetadataProvider::name(&integration),
                    seeded_by_default: integration.seeded_by_default(),
                    credential_types: CredentialTypeDiscriminants::iter()
                        .filter(|t| t.integration() == integration)
                        .map(|t| CredentialTypeEntry {
                            id: t.id(),
                            name: t.display_name(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}
