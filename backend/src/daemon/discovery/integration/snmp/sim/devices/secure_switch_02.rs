use std::net::Ipv4Addr;

use crate::daemon::discovery::integration::snmp::sim::mibs::BridgeTable;
use crate::daemon::discovery::integration::snmp::sim::tables::{IfRow, IfTable};
use crate::daemon::discovery::integration::snmp::sim::transport::Handler;
use crate::daemon::discovery::integration::snmp::sim::{Purpose, SimDevice, Tables};
use crate::daemon::discovery::integration::snmp::types::SystemInfo;
use crate::server::credentials::r#impl::types::{
    CredentialType, SnmpV3AuthProtocol, SnmpV3Security,
};

use super::inline;

pub fn device() -> SimDevice {
    SimDevice {
        name: "secure-switch-02",
        ip: Ipv4Addr::UNSPECIFIED,
        purpose: Purpose::Regression {
            issue: "SNMPv3 security levels",
            defect: "SNMPv3 AuthNoPriv with MD5 — a daemon that only builds AuthPriv SHA sessions cannot authenticate",
        },
        credential: CredentialType::SnmpV3 {
            security_name: "scanopymd5".into(),
            security: SnmpV3Security::AuthNoPriv {
                auth_protocol: SnmpV3AuthProtocol::Md5,
                auth_password: inline("md5authpass12345"),
            },
            context_name: None,
        },
        system: SystemInfo {
            sys_descr: Some("HPE OfficeConnect Switch 1820 24G J9980A, PT.02.11".into()),
            sys_object_id: Some("1.3.6.1.4.1.11.2.3.7.11.181".into()),
            sys_name: Some("secure-switch-02".into()),
            sys_location: Some("Branch Office, Comms Cupboard".into()),
            sys_contact: Some("netops@example.com".into()),
            sys_services: Some(2),
            sys_uptime: None,
            // Published from the ifTable at emission, never stored.
            if_number: None,
        },
        tables: Tables {
            if_table: Some(IfTable::new(vec![
                IfRow::port(1, "Port 1", Some("00:1a:2b:00:30:01".parse().unwrap()))
                    .alias("Uplink"),
                IfRow::port(2, "Port 2", Some("00:1a:2b:00:30:02".parse().unwrap())),
            ])),
            bridge: BridgeTable::derived(),
            ..Default::default()
        },
        arp_handler: Handler::Normal,
        suppresses: Vec::new(),
        rejects_getbulk: None,
    }
}
