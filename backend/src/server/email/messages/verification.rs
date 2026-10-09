use super::{Body, Content, Email, EmailCategory};

/// Sent after signup to confirm ownership of the email address via a
/// time-limited verification link.
pub struct Verification<'a> {
    pub url: &'a str,
    pub token: &'a str,
}

impl Email for Verification<'_> {
    fn subject(&self) -> String {
        "Verify Your Email".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "verification"
    }

    fn body_html(&self) -> String {
        let verify_url = format!(
            "{}/verify-email?token={}",
            self.url.trim_end_matches('/'),
            self.token
        );
        Body::new()
            .content(
                Content::new()
                    .heading("Verify Your Email")
                    .paragraph("Hi there,")
                    .paragraph("Thanks for signing up for Scanopy! Please verify your email address by clicking the button below:"),
            )
            .cta(&verify_url, "Verify Email")
            .alt_link(&verify_url)
            .notice(
                "Expiration Notice",
                "This verification link will expire in 24 hours. If you didn't create a Scanopy account, you can safely ignore this email.",
            )
            .render()
    }
}
