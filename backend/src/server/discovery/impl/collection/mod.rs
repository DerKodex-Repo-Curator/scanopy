//! What Scanopy collects, by category, and how each category is collected.
//!
//! [`CollectedData`] names the categories. Provenance names how: every collected value or row
//! records an [`AttributeSource`], and each source answers [`AttributeSource::collection`]. A
//! source either runs on every host with no credential (ARP, a port scan, reverse DNS, an
//! application probe), runs only through an integration credential, is a server-side inference
//! from data already collected, or is not collection at all (unattributed or entered by a person).
//! Integrations declare which sources they write into which categories
//! ([`CredentialIntegration::collection`]). The `collected-data` fixture lists every category with
//! its credential-free methods and the integrations that collect it, for the security page.
//!
//! The declarations are checked. [`present_in`] derives the `(category, source)` pairs a
//! submission carries, and [`undeclared_collection`] flags a pair its [`Producer`] does not
//! declare. The daemon runs that check on the network scan's hosts, on the daemon's report on its
//! own host, on every integration's result and on every host an integration submits, and each
//! integration's mapping tests run it on what the mapping builds.
//!
//! Some data reaches the server through calls other than a host submission, so the check cannot
//! see it and it rests on the declaration alone:
//! - VLAN records, which SNMP upserts through `DiscoveryOps::upsert_vlans`. Per-port VLAN
//!   membership on interfaces is checked.
//! - Subnets an integration creates with `DiscoveryOps::create_subnet` (SNMP routes, container
//!   bridge networks). Subnets added to the scanned host's `HostData` are checked.
//! - Services with no port, such as the service a controller integration matches from a managed
//!   device's type. Ports carry a source; these services carry only their match details.
//! - Connected clients reported by a controller, which arrive as hosts of their own. A client
//!   address read from another device's ARP cache or forwarding table counts as a connected
//!   client.
//!
//! [`AttributeSource`]: crate::server::shared::attribution::AttributeSource
//! [`AttributeSource::collection`]: crate::server::shared::attribution::AttributeSource::collection
//! [`CredentialIntegration::collection`]: crate::server::credentials::r#impl::types::CredentialIntegration::collection

mod check;
mod classify;
mod fixture;

pub use check::{
    present_in, report_undeclared_collection, undeclared_collection, undeclared_in_submitted_host,
};
pub use classify::{Producer, SourceCollection};
pub use fixture::{
    CollectedDataEntry, CollectedDataIntegrationEntry, CollectedDataMethodEntry,
    CredentialTypeEntry, all_collected_data,
};

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoStaticStr};
use utoipa::ToSchema;

use crate::server::shared::{
    concepts::Concept,
    types::{
        Color, Icon,
        metadata::{EntityMetadataProvider, HasId, TypeMetadataProvider},
    },
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
    ConnectedClients,
    Containers,
    VirtualMachines,
}

impl HasId for CollectedData {
    fn id(&self) -> &'static str {
        self.into()
    }
}

impl EntityMetadataProvider for CollectedData {
    fn color(&self) -> Color {
        match self {
            Self::Addresses | Self::Subnets => Concept::L3.color(),
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
