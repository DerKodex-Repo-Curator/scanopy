//! The field registry: what each entity type is described by. See `types/entity_field.rs`.
//!
//! Ids keep the column names CSV export used before the registry, and the list tables' column
//! keys, so neither saved column preferences nor scripts reading the CSV lose a column. An
//! `Entity` field whose id ends in `_id` is a foreign key: CSV writes the related entity's id.

use crate::server::shared::entities::EntityDiscriminants as E;
use crate::server::shared::types::entity_field::{
    EntityField as F, FieldFormat, Hop, MetadataFixture as Fx,
};

const SITE: F = F::new("site_id", "Site").entity(&[Hop::parent(E::Site, "site_id")]);
const SOURCE: F = F::new("source", "Source")
    .field("source.type")
    .metadata(Fx::EntitySources);
const DESCRIPTION: F = F::new("description", "Description");
const TIMESTAMP: FieldFormat = FieldFormat::Timestamp;

const HOST: &[F] = &[
    F::new("name", "Name").source("name_source").summary(),
    F::new("hostname", "Hostname")
        .source("hostname_source")
        .summary(),
    F::new("interface_ip", "IP Addresses")
        .via(
            &[Hop::children(E::IPAddress, "host_id")],
            Some("ip_address"),
        )
        .format(FieldFormat::Identifier)
        .summary(),
    F::new("interfaces", "Interfaces").entity(&[Hop::children(E::Interface, "host_id")]),
    F::new("services", "Services")
        .entity(&[Hop::children(E::Service, "host_id")])
        .summary(),
    F::new("ports", "Ports")
        .via(&[Hop::children(E::Port, "host_id")], None)
        .format(FieldFormat::Port),
    F::new("virtualized_by", "Virtualized By")
        .entity(&[Hop::parent(E::Service, "virtualization_service_id")])
        .summary(),
    F::new("presented_by", "Presented By")
        .entity(&[Hop::parent(E::Interface, "virtualization_interface_id")]),
    F::new("os_family", "Operating System")
        .field("os")
        .source("os_source")
        .format(FieldFormat::OperatingSystem)
        .summary(),
    F::new("manufacturer", "Manufacturer").source("manufacturer_source"),
    F::new("model", "Model").source("model_source"),
    F::new("serial_number", "Serial Number")
        .source("serial_number_source")
        .format(FieldFormat::Identifier),
    F::new("asset_tag", "Asset Tag")
        .source("asset_tag_source")
        .format(FieldFormat::Identifier),
    F::new("firmware_revision", "Firmware Revision")
        .source("firmware_revision_source")
        .format(FieldFormat::Identifier),
    F::new("software_revision", "Software Revision")
        .source("software_revision_source")
        .format(FieldFormat::Identifier),
    F::new("sys_name", "System Name").source("sys_name_source"),
    F::new("sys_descr", "System Description").source("sys_descr_source"),
    F::new("sys_object_id", "System OID")
        .source("sys_object_id_source")
        .format(FieldFormat::Identifier),
    F::new("sys_location", "Location").source("sys_location_source"),
    F::new("sys_contact", "Contact").source("sys_contact_source"),
    F::new("chassis_id", "Chassis ID")
        .source("chassis_id_source")
        .format(FieldFormat::Identifier),
    F::new("management_url", "Management URL").source("management_url_source"),
    DESCRIPTION,
    SOURCE,
    SITE,
];

const IP_ADDRESS: &[F] = &[
    F::new("ip_address", "IP Address")
        .source("ip_address_source")
        .format(FieldFormat::Identifier)
        .summary(),
    F::new("mac_address", "MAC Address")
        .source("mac_address_source")
        .format(FieldFormat::Identifier)
        .summary(),
    F::new("name", "Name"),
    F::new("host_id", "Host")
        .entity(&[Hop::parent(E::Host, "host_id")])
        .summary(),
    F::new("subnet_id", "Subnet")
        .entity(&[Hop::parent(E::Subnet, "subnet_id")])
        .summary(),
    F::new("services", "Services").entity(&[
        Hop::children(E::Binding, "ip_address_id"),
        Hop::parent(E::Service, "service_id"),
    ]),
    F::new("ports", "Ports")
        .via(
            &[
                Hop::children(E::Binding, "ip_address_id"),
                Hop::parent(E::Port, "port_id"),
            ],
            None,
        )
        .format(FieldFormat::Port),
    F::new("position", "Position").format(FieldFormat::Number),
    SITE,
];

const INTERFACE: &[F] = &[
    F::new("display_name", "Interface").summary(),
    F::new("host_id", "Host")
        .entity(&[Hop::parent(E::Host, "host_id")])
        .summary(),
    F::new("mac_address", "MAC Address")
        .source("mac_address_source")
        .format(FieldFormat::Identifier)
        .summary(),
    F::new("speed_bps", "Speed")
        .format(FieldFormat::BitsPerSecond)
        .summary(),
    F::new("oper_status", "Status")
        .metadata(Fx::IfOperStatuses)
        .summary(),
    F::new("admin_status", "Admin Status"),
    F::new("native_vlan_id", "Native VLAN")
        .entity(&[Hop::parent(E::Vlan, "native_vlan_id")])
        .source("vlan_membership_source")
        .summary(),
    F::new("vlan_ids", "Tagged VLANs")
        .entity(&[Hop::parent(E::Vlan, "vlan_ids")])
        .source("vlan_membership_source")
        .many(),
    F::new("ip_address_id", "IP Address").entity(&[Hop::parent(E::IPAddress, "ip_address_id")]),
    F::new("if_name", "ifName"),
    F::new("if_descr", "ifDescr"),
    F::new("if_alias", "Alias"),
    F::new("if_type", "ifType"),
    F::new("if_index", "Index").format(FieldFormat::Number),
    F::new("ip_configured", "IP Configured").format(FieldFormat::Boolean),
    F::new("fdb_macs", "Forwarding Table MACs")
        .source("fdb_macs_source")
        .format(FieldFormat::Identifier)
        .many(),
    SITE,
];

const SERVICE: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("service_definition", "Service Type")
        .metadata(Fx::ServiceDefinitions)
        .summary(),
    F::new("host", "Host")
        .entity(&[Hop::parent(E::Host, "host_id")])
        .summary(),
    F::new("port_bindings", "Port Bindings")
        .via(
            &[Hop::embedded("bindings"), Hop::parent(E::Port, "port_id")],
            None,
        )
        .format(FieldFormat::Port)
        .summary(),
    F::new("ip_bindings", "IP Address Bindings").entity(&[
        Hop::embedded("bindings"),
        Hop::parent(E::IPAddress, "ip_address_id"),
    ]),
    F::new("containerized_by", "Containerized By")
        .entity(&[Hop::parent(E::Service, "virtualization_service_id")]),
    F::new("position", "Position").format(FieldFormat::Number),
    SOURCE,
    SITE,
];

const PORT: &[F] = &[
    F::new("port_number", "Port")
        .field("number")
        .source("port_source")
        .format(FieldFormat::Number)
        .summary(),
    F::new("protocol", "Protocol").summary(),
    F::new("port_type", "Port Type").field("type"),
    F::new("host_id", "Host").entity(&[Hop::parent(E::Host, "host_id")]),
    SITE,
];

const BINDING: &[F] = &[
    F::new("binding_type", "Binding Type").field("type"),
    F::new("service_id", "Service").entity(&[Hop::parent(E::Service, "service_id")]),
    F::new("ip_address_id", "IP Address").entity(&[Hop::parent(E::IPAddress, "ip_address_id")]),
    F::new("port_id", "Port").entity(&[Hop::parent(E::Port, "port_id")]),
    SITE,
];

const SUBNET: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("cidr", "CIDR")
        .source("cidr_source")
        .format(FieldFormat::Identifier)
        .summary(),
    F::new("subnet_type", "Subnet Type")
        .metadata(Fx::SubnetTypes)
        .summary(),
    DESCRIPTION,
    F::new("managed_by", "Managed By")
        .entity(&[Hop::parent(E::Service, "virtualization_service_id")]),
    SOURCE,
    SITE,
];

const VLAN: &[F] = &[
    F::new("vlan_number", "VLAN Number")
        .format(FieldFormat::Number)
        .summary(),
    F::new("name", "Name").summary(),
    DESCRIPTION.summary(),
    F::new("subnet_ids", "Subnets")
        .entity(&[Hop::parent(E::Subnet, "subnet_ids")])
        .many(),
    F::new("interface_ids", "Interfaces")
        .entity(&[Hop::parent(E::Interface, "interface_ids")])
        .many(),
    SOURCE,
    SITE,
];

const DEPENDENCY: &[F] = &[
    F::new("name", "Name").summary(),
    DESCRIPTION.summary(),
    F::new("dependency_type", "Dependency Type")
        .metadata(Fx::DependencyTypes)
        .summary(),
    F::new("color", "Color"),
    SOURCE,
    SITE,
];

const TAG: &[F] = &[
    F::new("name", "Name").summary(),
    DESCRIPTION.summary(),
    F::new("color", "Color"),
    F::new("tag_group", "Tag Group").entity(&[Hop::parent(E::TagGroup, "tag_group_id")]),
    F::new("icon", "Icon"),
];

const TAG_GROUP: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("description", "Description"),
    F::new("group_type", "Group Type"),
    F::new("entity_types", "Applies To").many(),
];

const DAEMON: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("mode", "Daemon Mode"),
    F::new("version", "Version").summary(),
    F::new("last_seen", "Last Seen").format(TIMESTAMP),
    F::new("url", "URL"),
    F::new("host_id", "Host").entity(&[Hop::parent(E::Host, "host_id")]),
    F::new("maintainer", "Maintainer").entity(&[Hop::parent(E::User, "user_id")]),
    SITE,
];

const DISCOVERY: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("discovery_type", "Discovery Type")
        .field("discovery_type.type")
        .metadata(Fx::DiscoveryTypes)
        .summary(),
    F::new("run_type", "Run Type").field("run_type.type"),
    F::new("daemon_id", "Daemon").entity(&[Hop::parent(E::Daemon, "daemon_id")]),
    SITE,
];

const DAEMON_API_KEY: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("is_enabled", "Enabled").format(FieldFormat::Boolean),
    F::new("last_used", "Last Used").format(TIMESTAMP),
    F::new("expires_at", "Expires").format(TIMESTAMP),
    F::new("daemon_id", "Daemon").entity(&[Hop::parent(E::Daemon, "daemon_id")]),
    SITE,
];

const USER_API_KEY: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("permissions", "Permissions").metadata(Fx::Permissions),
    F::new("is_enabled", "Enabled").format(FieldFormat::Boolean),
    F::new("last_used", "Last Used").format(TIMESTAMP),
    F::new("expires_at", "Expires").format(TIMESTAMP),
    F::new("user_id", "User").entity(&[Hop::parent(E::User, "user_id")]),
];

const CREDENTIAL: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("credential_type", "Credential Type")
        .field("credential_type.type")
        .metadata(Fx::CredentialTypes)
        .summary(),
    DESCRIPTION,
    F::new("daemon_os", "Daemon OS"),
];

const SITE_FIELDS: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("stale_after_hours", "Stale After (hours)").format(FieldFormat::Number),
];

const USER: &[F] = &[
    F::new("email", "Email").summary(),
    F::new("permissions", "Permissions")
        .metadata(Fx::Permissions)
        .summary(),
    F::new("email_verified", "Email Verified").format(FieldFormat::Boolean),
    F::new("oidc_provider", "OIDC Provider"),
    F::new("terms_accepted_at", "Terms Accepted").format(TIMESTAMP),
];

const SHARE: &[F] = &[
    F::new("name", "Name").summary(),
    F::new("is_enabled", "Enabled").format(FieldFormat::Boolean),
    F::new("expires_at", "Expires").format(TIMESTAMP),
    F::new("topology_id", "Topology").entity(&[Hop::parent(E::Topology, "topology_id")]),
    F::new("created_by", "Created By").entity(&[Hop::parent(E::User, "created_by")]),
    SITE,
];

const TOPOLOGY: &[F] = &[SITE];
const ORGANIZATION: &[F] = &[F::new("name", "Name").summary()];
const SNAPSHOT: &[F] = &[
    F::new("taken_at", "Taken").format(TIMESTAMP).summary(),
    SITE,
];
const INVITE: &[F] = &[
    F::new("send_to", "Sent To").summary(),
    F::new("permissions", "Permissions").metadata(Fx::Permissions),
    F::new("expires_at", "Expires").format(TIMESTAMP),
];

impl E {
    /// The fields this entity type is described by, in display order.
    pub fn fields(&self) -> &'static [F] {
        match self {
            E::Site => SITE_FIELDS,
            E::Vlan => VLAN,
            E::Subnet => SUBNET,
            E::Host => HOST,
            E::Service => SERVICE,
            E::IPAddress => IP_ADDRESS,
            E::Interface => INTERFACE,
            E::Port => PORT,
            E::Binding => BINDING,
            E::Discovery => DISCOVERY,
            E::Daemon => DAEMON,
            E::DaemonApiKey => DAEMON_API_KEY,
            E::Tag => TAG,
            E::TagGroup => TAG_GROUP,
            E::User => USER,
            E::UserApiKey => USER_API_KEY,
            E::Credential => CREDENTIAL,
            E::Organization => ORGANIZATION,
            E::Invite => INVITE,
            E::Topology => TOPOLOGY,
            E::Snapshot => SNAPSHOT,
            E::Share => SHARE,
            E::Dependency => DEPENDENCY,
        }
    }

    /// Fields that name one entity in text, tried in order: what a text-only output writes for
    /// an `Entity` field. `display_name` is only present where the server computes one (hosts in
    /// a topology or a host response, interfaces).
    pub fn display_fields(&self) -> &'static [&'static str] {
        match self {
            E::Host => &["display_name", "name"],
            E::Interface => &["display_name", "if_name", "if_descr"],
            E::IPAddress => &["ip_address"],
            E::Port => &["number"],
            E::Vlan => &["vlan_number"],
            E::User | E::Invite => &["email", "send_to"],
            E::Binding => &["id"],
            E::Snapshot => &["taken_at"],
            E::Topology => &["id"],
            E::Site
            | E::Subnet
            | E::Service
            | E::Discovery
            | E::Daemon
            | E::DaemonApiKey
            | E::Tag
            | E::TagGroup
            | E::UserApiKey
            | E::Credential
            | E::Organization
            | E::Share
            | E::Dependency => &["name"],
        }
    }
}
