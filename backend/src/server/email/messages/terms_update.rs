use chrono::NaiveDate;

use super::{Body, Content, Email, EmailCategory};
use crate::server::release_notes::terms::{TermsDocument, TermsUpdate as Update};

/// Notice of a change to Scanopy's terms, sent by the cloud server to every
/// user who signed up before it takes effect, once per update.
pub struct TermsUpdate<'a> {
    pub update: &'a Update,
}

impl TermsUpdate<'_> {
    /// "December 1, 2026".
    fn effective(&self) -> String {
        self.update.effective.format("%B %-d, %Y").to_string()
    }

    /// "Terms of Service", "Terms of Service and Privacy Policy",
    /// "Terms of Service, Privacy Policy and Refund Policy".
    fn documents(&self) -> String {
        let names: Vec<&str> = self.update.documents.iter().map(|d| d.name()).collect();
        match names.split_last() {
            None => String::new(),
            Some((last, [])) => last.to_string(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        }
    }
}

fn document_link(document: TermsDocument, effective: NaiveDate) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        document.version_url(effective),
        document.name()
    )
}

impl Email for TermsUpdate<'_> {
    fn subject(&self) -> String {
        format!(
            "Scanopy's {} change on {}",
            self.documents(),
            self.effective()
        )
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Legal
    }

    fn campaign(&self) -> &'static str {
        "terms_update"
    }

    fn ses_message_tags(&self) -> Vec<(&'static str, String)> {
        vec![(
            "terms_version",
            self.update.effective.format("%Y-%m-%d").to_string(),
        )]
    }

    fn body_html(&self) -> String {
        let effective = self.effective();
        let links: Vec<String> = self
            .update
            .documents
            .iter()
            .map(|d| document_link(*d, self.update.effective))
            .collect();
        Body::new()
            .content(
                Content::new()
                    .heading(&format!("Changes to Scanopy's {}", self.documents()))
                    .paragraph(&format!(
                        "We're updating Scanopy's {}. The new versions take effect on {effective}.",
                        self.documents()
                    ))
                    .subheading("What changes")
                    .raw(&format!(
                        "<div style=\"margin: 0 0 24px 0; font-size: 16px; line-height: 24px; color: #4a4a4a;\">{}</div>\n",
                        self.update.html
                    ))
                    .paragraph(&format!("Read the full text: {}.", links.join(", ")))
                    .paragraph(&format!(
                        "If you keep using Scanopy after {effective}, the new versions apply to you. If you don't agree, cancel your subscription before that date."
                    )),
            )
            .render()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::release_notes::terms::tests::terms_file;

    #[test]
    fn subject_and_body_name_every_changed_document() {
        let (_, source) = terms_file("2026-12-01", "2026-10-20");
        let update = Update::parse(&source).unwrap();
        let email = TermsUpdate { update: &update };
        let html = email.body_html();
        for document in &update.documents {
            assert!(email.subject().contains(document.name()));
            assert!(html.contains(&document.version_url(update.effective)));
        }
        assert!(html.contains(&update.html));
    }
}
