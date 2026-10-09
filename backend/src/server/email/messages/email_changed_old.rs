use super::{Body, Content, Email, EmailCategory};

/// Sent to a user's previous email address as a security notice when the
/// account email is changed.
pub struct EmailChangedOld<'a> {
    pub new_email: &'a str,
}

impl Email for EmailChangedOld<'_> {
    fn subject(&self) -> String {
        "Your Email Was Changed".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Auth
    }

    fn campaign(&self) -> &'static str {
        "email_changed_old"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Email Address Changed")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "The email address on your Scanopy account was changed to <strong>{}</strong>.",
                        self.new_email
                    ))
                    .paragraph("If you made this change, no action is needed. If you didn't request this change, please contact support immediately."),
            )
            .render()
    }
}
