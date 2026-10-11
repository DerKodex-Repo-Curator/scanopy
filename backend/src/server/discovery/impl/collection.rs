//! What Scanopy collects, by category, and which sources collect each category.
//!
//! [`CollectedData`] names the categories. [`CollectionSource`] names where data comes from: the
//! network scan, the daemon's report on its own host, and each credential integration. Each source
//! declares the categories it collects ([`CollectionSource::collects`]), and the `collected-data`
//! fixture lists every category with its sources for the security page.
//!
//! The declaration is checked. [`CollectedData::present_in`] derives the categories a submission
//! carries from its fields, and [`undeclared_collection`] flags a category a source added without
//! declaring it. The daemon runs that check on every integration's result and on every host an
//! integration submits, and each integration's mapping tests run it on what the mapping builds.
//!
//! Some categories reach the server through calls other than a host submission, so the check
//! cannot see them and they rest on the declaration alone:
//! - VLANs, which SNMP upserts through `DiscoveryOps::upsert_vlans`. Per-port VLAN ids on
//!   interfaces are checked.
//! - Subnets an integration creates with `DiscoveryOps::create_subnet` (SNMP routes, container
//!   bridge networks). Subnets added to the scanned host's `HostData` are checked.
//! - Gateways, which the daemon reads from its routing table.
//! - Connected clients, which arrive as hosts of their own. Their fields are checked like any
//!   submitted host, but nothing marks a host as a client.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};
use utoipa::ToSchema;

use crate::server::{
    credentials::r#impl::types::CredentialIntegration,
    hosts::r#impl::{
        base::{Host, HostBase},
        virtualization::HostVirtualization,
    },
    interfaces::r#impl::base::{Interface, InterfaceBase},
    ip_addresses::r#impl::base::IPAddress,
    ports::r#impl::base::Port,
    services::r#impl::{base::Service, virtualization::ServiceVirtualization},
    shared::{
        concepts::Concept,
        types::{
            Color, Icon,
            metadata::{EntityMetadataProvider, HasId, TypeMetadataProvider},
        },
    },
    subnets::r#impl::base::Subnet,
};

/// A category of data Scanopy collects about a network. Variants are in display order.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    IntoStaticStr,
    EnumIter,
    ToSchema,
)]
pub enum CollectedData {
    Addresses,
    Hostnames,
    OperatingSystem,
    PortsAndServices,
    DeviceDetails,
    Interfaces,
    Neighbors,
    ForwardingTables,
    Vlans,
    Subnets,
    Gateways,
    ConnectedClients,
    Containers,
    VirtualMachines,
}

/// Where collected data comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionSource {
    /// The daemon's sweep of a subnet, with no credential: ARP, port scans, application probes,
    /// reverse DNS, mDNS and PROFINET DCP.
    NetworkScan,
    /// The daemon's report on the host it runs on.
    DaemonHost,
    /// A credential integration.
    Integration(CredentialIntegration),
}

impl CollectionSource {
    /// Every source: the network scan, the daemon host, then each integration in declaration
    /// order.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::NetworkScan, Self::DaemonHost]
            .into_iter()
            .chain(CredentialIntegration::iter().map(Self::Integration))
    }

    /// The categories this source collects. Exhaustive (no wildcard), and checked against what
    /// each integration actually submits (see the module docs).
    pub fn collects(&self) -> &'static [CollectedData] {
        use CollectedData::*;
        match self {
            // ARP sweep, port scan and app probes, reverse DNS and mDNS, OS from banners, industrial
            // probes (Modbus, EtherNet/IP) for device details, and the one-interface host PROFINET
            // DCP reports.
            Self::NetworkScan => &[
                Addresses,
                Hostnames,
                OperatingSystem,
                PortsAndServices,
                DeviceDetails,
                Interfaces,
            ],
            // Its own hostname, OS, addresses, NICs, lldpd neighbors, the daemon's service, the
            // subnets it sits on (macvlan and ipvlan networks included) and its routing table.
            Self::DaemonHost => &[
                Addresses,
                Hostnames,
                OperatingSystem,
                PortsAndServices,
                Interfaces,
                Neighbors,
                Subnets,
                Gateways,
            ],
            Self::Integration(integration) => integration.collects(),
        }
    }

    /// Whether this source runs only with a credential: every integration does, and the network
    /// scan and the daemon host never need one.
    pub fn requires_credential(&self) -> bool {
        match self {
            Self::NetworkScan | Self::DaemonHost => false,
            Self::Integration(_) => true,
        }
    }

    /// Whether this source is an integration with a seeded default credential, so it runs on new
    /// sites or daemons until the user unassigns it.
    pub fn seeded_by_default(&self) -> bool {
        match self {
            Self::NetworkScan | Self::DaemonHost => false,
            Self::Integration(integration) => integration.seeded_by_default(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::NetworkScan => "Network scan",
            Self::DaemonHost => "Daemon host",
            Self::Integration(integration) => TypeMetadataProvider::name(integration),
        }
    }
}

impl HasId for CollectionSource {
    fn id(&self) -> &'static str {
        match self {
            Self::NetworkScan => "NetworkScan",
            Self::DaemonHost => "DaemonHost",
            Self::Integration(integration) => integration.id(),
        }
    }
}

impl CollectedData {
    /// Every source that declares this category, in [`CollectionSource::all`] order.
    pub fn sources(&self) -> Vec<CollectionSource> {
        CollectionSource::all()
            .filter(|source| source.collects().contains(self))
            .collect()
    }

    /// The categories a host submission carries, derived from which of its fields hold data.
    ///
    /// Covers the categories a submission can express. Gateways and VLAN entities never appear
    /// here (see the module docs), and nothing marks a host as a connected client.
    ///
    /// The host and interface fields are destructured in full, so a new field does not compile
    /// until it is placed in a category or deliberately left out.
    pub fn present_in(
        host: &Host,
        ip_addresses: &[IPAddress],
        ports: &[Port],
        services: &[Service],
        interfaces: &[Interface],
        subnets: &[Subnet],
    ) -> BTreeSet<Self> {
        let mut present = BTreeSet::new();
        let mut mark = |category: Self, when: bool| {
            if when {
                present.insert(category);
            }
        };

        let HostBase {
            hostname,
            os,
            sys_descr,
            sys_object_id,
            sys_location,
            sys_contact,
            sys_name,
            manufacturer,
            model,
            serial_number,
            asset_tag,
            firmware_revision,
            software_revision,
            virtualization_metadata,
            // Identifiers and bookkeeping rather than collected data. The chassis id names the
            // device for neighbor matching; the management URL is where to reach it.
            name: _,
            site_id: _,
            description: _,
            source: _,
            virtualization_service_id: _,
            virtualization_interface_id: _,
            tags: _,
            management_url: _,
            chassis_id: _,
            credential_assignments: _,
        } = &host.base;

        mark(Self::Addresses, !ip_addresses.is_empty());
        mark(Self::Hostnames, hostname.is_some());
        mark(Self::OperatingSystem, os.is_some());
        mark(
            Self::PortsAndServices,
            !ports.is_empty() || !services.is_empty(),
        );
        mark(
            Self::DeviceDetails,
            sys_descr.is_some()
                || sys_object_id.is_some()
                || sys_location.is_some()
                || sys_contact.is_some()
                || sys_name.is_some()
                || manufacturer.is_some()
                || model.is_some()
                || serial_number.is_some()
                || asset_tag.is_some()
                || firmware_revision.is_some()
                || software_revision.is_some(),
        );
        mark(Self::Interfaces, !interfaces.is_empty());
        mark(Self::Subnets, !subnets.is_empty());

        if let Some(virtualization) = virtualization_metadata {
            match virtualization {
                HostVirtualization::Docker(_) | HostVirtualization::Podman(_) => {
                    mark(Self::Containers, true)
                }
                HostVirtualization::Proxmox(_)
                | HostVirtualization::VCenter(_)
                | HostVirtualization::ESXi(_) => mark(Self::VirtualMachines, true),
                // An address the host presents from an interface of its own. Its addresses
                // already count.
                HostVirtualization::NetworkIdentity(_) => {}
            }
        }

        for service in services {
            if let Some(virtualization) = &service.base.virtualization_metadata {
                match virtualization {
                    ServiceVirtualization::Docker(_) | ServiceVirtualization::Podman(_) => {
                        mark(Self::Containers, true)
                    }
                }
            }
        }

        for interface in interfaces {
            let InterfaceBase {
                neighbor_candidates,
                fdb_macs,
                native_vlan_id,
                vlan_ids,
                // The interface row itself, counted as Interfaces above.
                host_id: _,
                site_id: _,
                if_index: _,
                if_descr: _,
                if_name: _,
                if_alias: _,
                if_type: _,
                speed_bps: _,
                admin_status: _,
                oper_status: _,
                mac_address: _,
                ip_address_id: _,
                ip_configured: _,
            } = &interface.base;
            mark(Self::Neighbors, !neighbor_candidates.is_empty());
            mark(
                Self::ForwardingTables,
                fdb_macs.as_ref().is_some_and(|m| !m.is_empty()),
            );
            mark(
                Self::Vlans,
                native_vlan_id.is_some() || vlan_ids.as_ref().is_some_and(|v| !v.is_empty()),
            );
        }

        present
    }
}

/// The first category present in `after` but not in `before` that `source` does not declare
/// collecting, or `None` when everything it added is declared.
///
/// A category already present in `before` is never flagged, so a source that adds more of a
/// category another source already filled goes unchecked for that category. Checking each
/// integration's mapping on its own, from an empty host, covers that case in tests.
pub fn undeclared_collection(
    source: CollectionSource,
    before: &BTreeSet<CollectedData>,
    after: &BTreeSet<CollectedData>,
) -> Option<CollectedData> {
    after
        .difference(before)
        .copied()
        .find(|category| !source.collects().contains(category))
}

/// The first category a host submitted by `integration` carries that the integration does not
/// declare collecting.
///
/// The host is new, so everything on it counts as collected, except subnets: those only place the
/// addresses and are the site's own, so a submission does not take them.
pub fn undeclared_in_submitted_host(
    integration: CredentialIntegration,
    host: &Host,
    ip_addresses: &[IPAddress],
    ports: &[Port],
    services: &[Service],
    interfaces: &[Interface],
) -> Option<CollectedData> {
    undeclared_collection(
        CollectionSource::Integration(integration),
        &BTreeSet::new(),
        &CollectedData::present_in(host, ip_addresses, ports, services, interfaces, &[]),
    )
}

/// Fail a debug assertion, and warn in a release build, for a category `source` collected without
/// declaring it. Release builds keep the data: the gap is in the declaration, and in the security
/// page built from it, not in what was collected.
pub fn report_undeclared_collection(source: CollectionSource, undeclared: Option<CollectedData>) {
    if let Some(category) = undeclared {
        tracing::warn!(
            source = source.id(),
            category = category.id(),
            "A collection source submitted a category of data it does not declare collecting"
        );
        debug_assert!(
            false,
            "{source:?} collected {category:?}; add it to its collects() declaration"
        );
    }
}

impl HasId for CollectedData {
    fn id(&self) -> &'static str {
        self.into()
    }
}

impl EntityMetadataProvider for CollectedData {
    fn color(&self) -> Color {
        match self {
            Self::Addresses | Self::Subnets | Self::Gateways => Concept::L3.color(),
            Self::Interfaces | Self::Neighbors | Self::ForwardingTables | Self::Vlans => {
                Concept::L2.color()
            }
            Self::Containers => Concept::Containerization.color(),
            Self::VirtualMachines => Concept::Virtualization.color(),
            Self::Hostnames
            | Self::OperatingSystem
            | Self::PortsAndServices
            | Self::DeviceDetails
            | Self::ConnectedClients => Color::Gray,
        }
    }

    fn icon(&self) -> Icon {
        match self {
            Self::Addresses => Icon::MapPin,
            Self::Hostnames => Icon::Tag,
            Self::OperatingSystem => Icon::Monitor,
            Self::PortsAndServices => Icon::Layers,
            Self::DeviceDetails => Icon::Info,
            Self::Interfaces => Icon::EthernetPort,
            Self::Neighbors => Icon::Cable,
            Self::ForwardingTables => Icon::Table,
            Self::Vlans => Icon::Split,
            Self::Subnets => Icon::Network,
            Self::Gateways => Icon::Router,
            Self::ConnectedClients => Icon::Users,
            Self::Containers => Concept::Containerization.icon(),
            Self::VirtualMachines => Concept::Virtualization.icon(),
        }
    }
}

impl TypeMetadataProvider for CollectedData {
    fn name(&self) -> &'static str {
        match self {
            Self::Addresses => "Addresses",
            Self::Hostnames => "Hostnames",
            Self::OperatingSystem => "Operating system",
            Self::PortsAndServices => "Ports and services",
            Self::DeviceDetails => "Device details",
            Self::Interfaces => "Interfaces",
            Self::Neighbors => "Neighbors",
            Self::ForwardingTables => "Forwarding tables",
            Self::Vlans => "VLANs",
            Self::Subnets => "Subnets",
            Self::Gateways => "Gateways",
            Self::ConnectedClients => "Connected clients",
            Self::Containers => "Containers",
            Self::VirtualMachines => "Virtual machines",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Addresses => "IP addresses and the MAC address seen at each.",
            Self::Hostnames => "Hostnames from DNS, mDNS, DHCP or the host itself.",
            Self::OperatingSystem => "Operating system family and version.",
            Self::PortsAndServices => "Open ports and the services identified on a host.",
            Self::DeviceDetails => {
                "Manufacturer, model, serial number, firmware and software revision, asset tag, \
                 and the SNMP system description, object ID, name, location and contact."
            }
            Self::Interfaces => {
                "Network interfaces with their names, descriptions, speed, status and MAC address."
            }
            Self::Neighbors => "LLDP and CDP neighbors, and the uplinks between devices.",
            Self::ForwardingTables => "The MAC addresses a switch has learned on each port.",
            Self::Vlans => "VLANs, and the native and tagged VLANs on each port.",
            Self::Subnets => "Subnets and container networks a host has addresses in.",
            Self::Gateways => "Default gateways from the daemon host's routing table.",
            Self::ConnectedClients => {
                "Hosts a device or controller reports seeing: ARP cache entries and controller clients."
            }
            Self::Containers => "Containers, the services they run and the ports they publish.",
            Self::VirtualMachines => {
                "Virtual machines and LXC containers, and the node running each."
            }
        }
    }
}

/// One category in the `collected-data` fixture, with every source that declares it.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub sources: Vec<CollectedDataSourceEntry>,
}

/// A source in a [`CollectedDataEntry`], by id and display name.
#[derive(Debug, Clone, Serialize)]
pub struct CollectedDataSourceEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// [`CollectionSource::requires_credential`].
    pub requires_credential: bool,
    /// [`CollectionSource::seeded_by_default`].
    pub seeded_by_default: bool,
}

/// Every category in display order, each with the sources that declare it. Emitted as
/// `collected-data.json` by `generate-fixtures`.
pub fn all_collected_data() -> Vec<CollectedDataEntry> {
    CollectedData::iter()
        .map(|category| CollectedDataEntry {
            id: category.id(),
            name: TypeMetadataProvider::name(&category),
            description: category.description(),
            sources: category
                .sources()
                .iter()
                .map(|source| CollectedDataSourceEntry {
                    id: source.id(),
                    name: source.name(),
                    requires_credential: source.requires_credential(),
                    seeded_by_default: source.seeded_by_default(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A category no source declares would be listed on the security page with nothing behind it.
    #[test]
    fn every_category_has_a_source() {
        for category in CollectedData::iter() {
            assert!(
                !category.sources().is_empty(),
                "{category:?} is declared by no source"
            );
        }
    }

    /// Source ids are unique, so the fixture can key on them.
    #[test]
    fn source_ids_are_unique() {
        let ids: Vec<&str> = CollectionSource::all().map(|s| s.id()).collect();
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "{ids:?}");
    }

    #[test]
    fn a_category_already_present_is_not_charged_to_the_source() {
        let source = CollectionSource::Integration(CredentialIntegration::Gnmi);
        let before = BTreeSet::from([CollectedData::Hostnames]);
        let after = BTreeSet::from([CollectedData::Hostnames, CollectedData::Interfaces]);
        assert_eq!(undeclared_collection(source, &before, &after), None);
    }

    #[test]
    fn a_category_added_without_a_declaration_is_flagged() {
        let source = CollectionSource::Integration(CredentialIntegration::Gnmi);
        let after = BTreeSet::from([CollectedData::Interfaces, CollectedData::Hostnames]);
        assert_eq!(
            undeclared_collection(source, &BTreeSet::new(), &after),
            Some(CollectedData::Hostnames)
        );
    }

    /// A default host carries no category, so a default field value never charges a category to
    /// every source that submits a host.
    #[test]
    fn an_empty_host_carries_nothing() {
        let host = Host::new(HostBase::default());
        assert!(CollectedData::present_in(&host, &[], &[], &[], &[], &[]).is_empty());
    }
}
