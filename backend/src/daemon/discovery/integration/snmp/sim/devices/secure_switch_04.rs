use std::net::Ipv4Addr;

use crate::daemon::discovery::integration::snmp::sim::mibs::BridgeTable;
use crate::daemon::discovery::integration::snmp::sim::tables::{IfRow, IfTable};
use crate::daemon::discovery::integration::snmp::sim::transport::Handler;
use crate::daemon::discovery::integration::snmp::sim::{Purpose, SimDevice, Tables};
use crate::daemon::discovery::integration::snmp::types::SystemInfo;
use crate::server::credentials::r#impl::types::{
    CredentialType, SnmpV3AuthProtocol, SnmpV3PrivProtocol, SnmpV3Security,
};

use super::inline;

pub fn device() -> SimDevice {
    SimDevice {
        name: "secure-switch-04",
        ip: Ipv4Addr::UNSPECIFIED,
        purpose: Purpose::Regression {
            issue: "SNMPv3 security levels",
            defect: "SNMPv3 AuthPriv with MD5 auth — a daemon that only offers SHA cannot authenticate",
        },
        credential: CredentialType::SnmpV3 {
            security_name: "scanopymd5priv".into(),
            security: SnmpV3Security::AuthPriv {
                auth_protocol: SnmpV3AuthProtocol::Md5,
                auth_password: inline("md5authpass12345"),
                priv_protocol: SnmpV3PrivProtocol::Aes128,
                priv_password: inline("md5privpass12345"),
            },
            context_name: None,
        },
        system: SystemInfo {
            sys_descr: Some(
                "Cisco IOS Software, C2960 Software (C2960-LANBASEK9-M), Version 12.2(55)SE12"
                    .into(),
            ),
            sys_object_id: Some("1.3.6.1.4.1.9.1.1208".into()),
            sys_name: Some("secure-switch-04".into()),
            sys_location: Some("Warehouse, Rack 2".into()),
            sys_contact: Some("netops@example.com".into()),
            sys_services: Some(2),
            sys_uptime: None,
            // Published from the ifTable at emission, never stored.
            if_number: None,
        },
        tables: Tables {
            if_table: Some(IfTable::new(vec![
                IfRow::port(1, "Fa0/1", Some("00:1a:2b:00:32:01".parse().unwrap())).alias("Uplink"),
                IfRow::port(2, "Fa0/2", Some("00:1a:2b:00:32:02".parse().unwrap())),
            ])),
            bridge: BridgeTable::derived(),
            ..Default::default()
        },
        arp_handler: Handler::Normal,
        suppresses: Vec::new(),
        rejects_getbulk: None,
    }
}
