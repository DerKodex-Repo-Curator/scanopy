use std::net::Ipv4Addr;

use crate::daemon::discovery::integration::snmp::sim::mibs::BridgeTable;
use crate::daemon::discovery::integration::snmp::sim::tables::{IfRow, IfTable};
use crate::daemon::discovery::integration::snmp::sim::transport::Handler;
use crate::daemon::discovery::integration::snmp::sim::{Purpose, SimDevice, Tables};
use crate::daemon::discovery::integration::snmp::types::SystemInfo;
use crate::server::credentials::r#impl::types::{CredentialType, SnmpV3Security};

pub fn device() -> SimDevice {
    SimDevice {
        name: "secure-switch-03",
        ip: Ipv4Addr::UNSPECIFIED,
        purpose: Purpose::Regression {
            issue: "SNMPv3 security levels",
            defect: "SNMPv3 NoAuthNoPriv — a user name and nothing else, which a daemon that requires an auth password cannot express",
        },
        credential: CredentialType::SnmpV3 {
            security_name: "scanopynoauth".into(),
            security: SnmpV3Security::NoAuthNoPriv,
            context_name: None,
        },
        system: SystemInfo {
            sys_descr: Some("Linux lab-pdu-01 4.14.0 armv7l".into()),
            sys_object_id: Some("1.3.6.1.4.1.8072.3.2.10".into()),
            sys_name: Some("secure-switch-03".into()),
            sys_location: Some("Server Room A, Rack 4".into()),
            sys_contact: Some("netops@example.com".into()),
            sys_services: Some(2),
            sys_uptime: None,
            // Published from the ifTable at emission, never stored.
            if_number: None,
        },
        tables: Tables {
            if_table: Some(IfTable::new(vec![
                IfRow::port(1, "eth0", Some("00:1a:2b:00:31:01".parse().unwrap())),
                IfRow::port(2, "eth1", Some("00:1a:2b:00:31:02".parse().unwrap())),
            ])),
            bridge: BridgeTable::derived(),
            ..Default::default()
        },
        arp_handler: Handler::Normal,
        suppresses: Vec::new(),
        rejects_getbulk: None,
    }
}
