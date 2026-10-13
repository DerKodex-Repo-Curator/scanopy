use crate::daemon::utils::app_probe::AppProbe;
use crate::daemon::utils::app_probe::snmp_engine::SnmpEngineProbe;
use crate::server::services::definitions::{ServiceDefinitionFactory, create_service};
use crate::server::services::r#impl::categories::ServiceCategory;
use crate::server::services::r#impl::definitions::ServiceDefinition;
use crate::server::services::r#impl::patterns::{ClientProbe, Pattern, probe_pattern};

#[derive(Default, Clone, Eq, PartialEq, Hash)]
pub struct Snmp;

impl ServiceDefinition for Snmp {
    fn name(&self) -> &'static str {
        "SNMP"
    }
    fn description(&self) -> &'static str {
        "Simple Network Management Protocol"
    }
    fn category(&self) -> ServiceCategory {
        ServiceCategory::NetworkCore
    }

    /// Either a credential got an answer, or the agent answered SNMPv3 engine discovery, which
    /// needs none. A bare open port is never enough: a v1/v2c agent drops a wrong community
    /// silently, and an unanswered UDP port looks the same open or filtered.
    fn discovery_pattern(&self) -> Pattern<'_> {
        Pattern::AnyOf(vec![
            Pattern::ClientResponse(ClientProbe::Snmp),
            probe_pattern(&SnmpEngineProbe),
        ])
    }
    fn app_probes(&self) -> Vec<Box<dyn AppProbe>> {
        vec![Box::new(SnmpEngineProbe)]
    }
    fn is_generic(&self) -> bool {
        true
    }
}

inventory::submit!(ServiceDefinitionFactory::new(create_service::<Snmp>));
