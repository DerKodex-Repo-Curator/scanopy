//! SNMP detection with SNMPv3 engine discovery, which needs no credential.
//!
//! Before a v3 manager can authenticate it has to learn the agent's engine id, so RFC 3414 §4 has
//! the agent answer a keyless request carrying an empty engine id with a Report
//! (`usmStatsUnknownEngineIDs`) naming its engine. Any agent whose software implements v3 answers
//! it, whatever versions its configuration grants access to: net-snmp's `snmpd` configured with
//! only a v2c community still does. So this finds the agent, and says nothing about which version
//! or community it accepts. Nothing unauthenticated can: a wrong community is dropped without a
//! word.
//!
//! Agents whose software lacks v3, or has it switched off, stay invisible. Those are still found by
//! the SNMP integration once a community answers.
//!
//! This is evidence that an SNMP agent answered, not that a credential worked, so it has its own
//! [`ClientProbe::SnmpEngine`] and leaves [`ClientProbe::Snmp`] meaning the latter.

use anyhow::Error;
use async_trait::async_trait;
use snmp2::AsyncSession;
use snmp2::v3::{Auth, Security};
use std::net::SocketAddr;
use tokio::time::{Duration, timeout};

use crate::daemon::utils::app_probe::{AppProbe, AppProbeOutcome, ProbeContext, presence};
use crate::server::ports::r#impl::base::PortType;
use crate::server::services::r#impl::patterns::ClientProbe;

/// How long to wait for the Report. As long as IKE's: agents on busy network gear answer slowly.
const ENGINE_DISCOVERY_TIMEOUT: Duration = Duration::from_millis(2000);

pub struct SnmpEngineProbe;

#[async_trait]
impl AppProbe for SnmpEngineProbe {
    fn port(&self) -> PortType {
        PortType::Snmp
    }

    fn client_probe(&self) -> Option<ClientProbe> {
        Some(ClientProbe::SnmpEngine)
    }

    async fn run(&self, ctx: &ProbeContext) -> Result<AppProbeOutcome, Error> {
        Ok(presence(
            discovers_engine(SocketAddr::new(ctx.ip, self.port().number())).await,
        ))
    }
}

/// Whether an agent at `addr` answered engine discovery with its engine id.
///
/// snmp2's `init` is that exchange: it sends the keyless request and fails unless the reply set an
/// engine id, so a stray or malformed datagram does not count.
async fn discovers_engine(addr: SocketAddr) -> bool {
    let security = Security::new(b"", b"").with_auth(Auth::NoAuthNoPriv);
    let Ok(mut session) = AsyncSession::new_v3(addr, rand::random::<u16>().into(), security).await
    else {
        return false;
    };
    matches!(
        timeout(ENGINE_DISCOVERY_TIMEOUT, session.init()).await,
        Ok(Ok(()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::discovery::integration::snmp::sim::{self, usm::UsmAgent};
    use std::net::Ipv4Addr;
    use tokio::net::UdpSocket;

    #[tokio::test]
    async fn an_agent_answers_engine_discovery_without_a_credential() {
        let device = sim::device("secure-switch-01");
        let agent = UsmAgent::start(&device).await;

        assert!(discovers_engine(SocketAddr::new(agent.ip(), agent.port())).await);
    }

    /// A v1/v2c-only agent, or a filtered port, stays silent.
    #[tokio::test]
    async fn silence_is_no_agent() {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = socket.local_addr().unwrap();

        assert!(!discovers_engine(addr).await);
        drop(socket);
    }
}
