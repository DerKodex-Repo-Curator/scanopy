use super::{Body, Content, Email, EmailCategory};

/// Sent as a security notice when an OIDC login provider is unlinked from a
/// user's account.
pub struct OidcUnlinked<'a> {
    pub provider_name: &'a str,
}

impl Email for OidcUnlinked<'_> {
    fn subject(&self) -> String {
        format!("{} Login Disconnected", self.provider_name)
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "oidc_unlinked"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Login Method Disconnected")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Your {0} account has been unlinked from your Scanopy account. You can no longer sign in using {0}.",
                        self.provider_name
                    ))
                    .paragraph("If you didn't make this change, please sign in to your account and review your security settings."),
            )
            .render()
    }
}
