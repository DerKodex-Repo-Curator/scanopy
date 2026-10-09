use super::{Body, Content, Email, EmailCategory};

/// Sent when a user requests a password reset, providing a time-limited link
/// to choose a new password.
pub struct PasswordReset<'a> {
    pub url: &'a str,
    pub token: &'a str,
}

impl Email for PasswordReset<'_> {
    fn subject(&self) -> String {
        "Password Reset".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "password_reset"
    }

    fn body_html(&self) -> String {
        let reset_url = format!(
            "{}/login?token={}",
            self.url.trim_end_matches('/'),
            self.token
        );
        Body::new()
            .content(
                Content::new()
                    .heading("Reset Your Password")
                    .paragraph("Hi there,")
                    .paragraph("We received a request to reset your password for your Scanopy account. Click the button below to create a new password:"),
            )
            .cta(&reset_url, "Reset Password")
            .alt_link(&reset_url)
            .notice(
                "Security Notice",
                "This password reset link will expire in 24 hours. If you didn't request a password reset, you can safely ignore this email.",
            )
            .render()
    }
}
