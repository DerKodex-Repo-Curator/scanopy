use crate::server::credentials::r#impl::integrations::{all_integrations, join_oxford};

use super::{Body, Content, Email, EmailCategory, links};

/// Post-daemon-registration walkthrough.
pub struct DiscoveryGuide<'a> {
    pub daemon_name: &'a str,
    pub site_name: &'a str,
}

impl DiscoveryGuide<'_> {
    /// Every integration, by name. Lists all of them rather than the org's own: the email goes
    /// out when the first daemon registers, before the org has added a credential.
    fn integration_names() -> String {
        let names: Vec<String> = all_integrations().into_iter().map(|i| i.name).collect();
        join_oxford(&names)
    }
}

impl Email for DiscoveryGuide<'_> {
    fn subject(&self) -> String {
        "Your Daemon is Connected - Discovery is Running".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::ProductOnboarding
    }

    fn campaign(&self) -> &'static str {
        "discovery_guide"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Your Daemon is Connected!")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Great news — your daemon <strong>{}</strong> just registered on <strong>{}</strong>. Scanopy is now running an initial discovery to map out your network.",
                        self.daemon_name, self.site_name
                    ))
                    .paragraph("Here's what happens next:")
                    .raw(&format!(
r#"                            <ul style="margin: 0 0 20px 0; padding-left: 20px; font-size: 16px; line-height: 28px; color: #4a4a4a;">
                                <li><strong>Self-report:</strong> The daemon host's own services and IP addresses are mapped automatically.</li>
                                <li><strong>Site scan:</strong> Scanopy scans your local subnets for other hosts, ports, and services.</li>
                                <li><strong>Topology:</strong> Once discovery finishes, your interactive topology map will be ready.</li>
                                <li><strong>Integrations:</strong> Add a credential and discovery also reads from {}.</li>
                            </ul>
"#,
                        Self::integration_names()
                    )),
            )
            .cta(links::APP_HOME, "Open Scanopy")
            .render()
    }
}
