use crate::server::ports::r#impl::base::PortType;
use crate::server::services::definitions::{ServiceDefinitionFactory, create_service};
use crate::server::services::r#impl::categories::ServiceCategory;
use crate::server::services::r#impl::definitions::ServiceDefinition;
use crate::server::services::r#impl::patterns::Pattern;

#[derive(Default, Clone, Eq, PartialEq, Hash)]
pub struct Calibre;

impl ServiceDefinition for Calibre {
    fn name(&self) -> &'static str {
        "Calibre"
    }
    fn description(&self) -> &'static str {
        "The calibre e-book manager's content server."
    }
    fn category(&self) -> ServiceCategory {
        ServiceCategory::Media
    }

    /// The content server stamps `Server: calibre <version>` on every response, and its
    /// unauthenticated index page carries a fixed sentence that survives a reverse proxy rewriting
    /// that header. It defaults to 8080; the linuxserver.io image serves it on 8081 and puts the
    /// Selkies desktop GUI on 8080, which says nothing about calibre.
    fn discovery_pattern(&self) -> Pattern<'_> {
        Pattern::AnyOf(
            [PortType::Http8080, PortType::Http8081]
                .into_iter()
                .flat_map(|port| {
                    [
                        Pattern::Header(Some(port), "server", "calibre", None),
                        Pattern::Endpoint(port, "/", "non-JavaScript version of this site", None),
                    ]
                })
                .collect(),
        )
    }

    fn logo_url(&self) -> &'static str {
        "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/calibre.svg"
    }
}

inventory::submit!(ServiceDefinitionFactory::new(create_service::<Calibre>));
