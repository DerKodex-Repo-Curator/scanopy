use super::{Body, Content, Email, EmailCategory};

/// Sent to invite a recipient to join an existing Scanopy organization.
pub struct Invite<'a> {
    pub url: &'a str,
    pub inviter: &'a str,
}

impl Email for Invite<'_> {
    fn subject(&self) -> String {
        format!("You've been invited to join {} on Scanopy", self.inviter)
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Invitation
    }

    fn campaign(&self) -> &'static str {
        "invite"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("You've Been Invited to Scanopy")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "{} has invited you to join their Scanopy instance to visualize and explore their network infrastructure.",
                        self.inviter
                    ))
                    .paragraph("Click the button below to accept the invitation and create your account:"),
            )
            .cta(self.url, "Accept Invitation")
            .alt_link(self.url)
            .notice(
                "Expiration Notice",
                "This invitation link will expire in 7 days. If you didn't expect this invitation, you can safely ignore this email.",
            )
            .render()
    }
}
