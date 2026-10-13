use crate::daemon::utils::app_probe::AppProbe;
use crate::daemon::utils::app_probe::docker_ping::PodmanPingProbe;
use crate::server::services::definitions::{ServiceDefinitionFactory, create_service};
use crate::server::services::r#impl::categories::ServiceCategory;
use crate::server::services::r#impl::definitions::ServiceDefinition;
use crate::server::services::r#impl::patterns::{ClientProbe, Pattern, probe_pattern};

#[derive(Default, Clone, Eq, PartialEq, Hash)]
pub struct Podman;

impl ServiceDefinition for Podman {
    fn name(&self) -> &'static str {
        "Podman"
    }
    fn description(&self) -> &'static str {
        "Daemonless container runtime"
    }
    fn category(&self) -> ServiceCategory {
        ServiceCategory::ContainerRuntime
    }

    /// Either a credential reached the socket or proxy, or Podman's API service answered `/_ping`
    /// on TCP 8888, the `podman system service` man page's example, without one. The default
    /// socket-only install has no network signal.
    fn discovery_pattern(&self) -> Pattern<'_> {
        Pattern::AnyOf(vec![
            Pattern::ClientResponse(ClientProbe::Podman),
            probe_pattern(&PodmanPingProbe),
        ])
    }
    fn app_probes(&self) -> Vec<Box<dyn AppProbe>> {
        vec![Box::new(PodmanPingProbe)]
    }

    fn logo_url(&self) -> &'static str {
        "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/podman.svg"
    }
}

inventory::submit!(ServiceDefinitionFactory::new(create_service::<Podman>));
