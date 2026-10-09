use super::{Body, Content, Email, EmailCategory};

/// Sent as a security notice when an OIDC login provider is linked to a
/// user's account.
pub struct OidcLinked<'a> {
    pub provider_name: &'a str,
}

impl Email for OidcLinked<'_> {
    fn subject(&self) -> String {
        format!("{} Login Connected", self.provider_name)
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "oidc_linked"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Login Method Connected")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Your {0} account has been linked to your Scanopy account. You can now sign in using {0}.",
                        self.provider_name
                    ))
                    .paragraph("If you didn't make this change, please sign in to your account and unlink this provider from Settings."),
            )
            .render()
    }
}
