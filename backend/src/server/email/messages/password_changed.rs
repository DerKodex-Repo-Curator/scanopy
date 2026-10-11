use super::{Body, Content, Email, EmailCategory};

/// Sent as a security notice after a user's account password is changed.
pub struct PasswordChanged<'a> {
    pub timestamp: &'a str,
}

impl Email for PasswordChanged<'_> {
    fn subject(&self) -> String {
        "Your Password Was Changed".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "password_changed"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Password Changed")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Your Scanopy password was changed on {}.",
                        self.timestamp
                    ))
                    .paragraph("If you made this change, no action is needed. If you didn't change your password, please reset it immediately and contact support."),
            )
            .render()
    }
}
