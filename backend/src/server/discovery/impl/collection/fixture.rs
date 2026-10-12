//! The `collected-data` fixture: every category, how it is collected without a credential, and
//! which integrations collect it.

use serde::Serialize;
use strum::IntoEnumIterator;

use crate::server::shared::{
    attribution::{AttributeSource, AttributeSourceDiscriminants},
    types::metadata::{HasId, TypeMetadataProvider},
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
    /// The source the data is recorded with, or `"Probe"` for the protocol probes taken together.
    pub source: serde_json::Value,
    /// [`AttributeSource::label`], or [`PROTOCOL_PROBES`] for the protocol probes.
    pub name: String,
    /// [`AttributeSource::describe`], or [`PROTOCOL_PROBES_DESCRIPTION`].
    pub description: String,
    /// The protocols of a protocol-probe entry, by [`ClientProbe`] name. Empty otherwise.
    pub protocols: Vec<&'static str>,
}

/// One method for every `Probe(p)` source in a category: each is the same exchange, a session in
/// the service's own protocol on an open port, and they differ only in the protocol.
const PROTOCOL_PROBES: &str = "Protocol probes";
const PROTOCOL_PROBES_DESCRIPTION: &str = "The scan opened a session in the service's own protocol on an open port, and the service answered.";

/// A category's credential-free methods, with its `Probe(p)` sources folded into one entry.
fn method_entries(category: CollectedData) -> Vec<CollectedDataMethodEntry> {
    let mut entries = Vec::new();
    let mut protocols = Vec::new();
    for source in credential_free_methods(category) {
        match source {
            AttributeSource::Probe(probe) => protocols.push(TypeMetadataProvider::name(&probe)),
            _ => entries.push(CollectedDataMethodEntry {
                source: serde_json::to_value(source).unwrap_or_default(),
                name: source.label(),
                description: source.describe(),
                protocols: Vec::new(),
            }),
        }
    }
    if !protocols.is_empty() {
        entries.push(CollectedDataMethodEntry {
            source: serde_json::Value::from(<&'static str>::from(
                AttributeSourceDiscriminants::Probe,
            )),
            name: PROTOCOL_PROBES.to_string(),
            description: PROTOCOL_PROBES_DESCRIPTION.to_string(),
            protocols,
        });
    }
    entries
}

/// An integration that collects a category.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataIntegrationEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// Whether the integration has a seeded default credential, so it runs on new sites or
    /// daemons until the user unassigns it.
    pub seeded_by_default: bool,
}

/// Every category in display order. Emitted as `collected-data.json` by `generate-fixtures`.
pub fn all_collected_data() -> Vec<CollectedDataEntry> {
    CollectedData::iter()
        .map(|category| CollectedDataEntry {
            id: category.id(),
            name: TypeMetadataProvider::name(&category),
            description: category.description(),
            methods: method_entries(category),
            integrations: collecting_integrations(category)
                .into_iter()
                .map(|integration| CollectedDataIntegrationEntry {
                    id: integration.id(),
                    name: TypeMetadataProvider::name(&integration),
                    seeded_by_default: integration.seeded_by_default(),
                })
                .collect(),
        })
        .collect()
}
