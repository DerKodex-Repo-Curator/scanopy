use super::{Body, Content, Email, EmailCategory, compose::html_escape, links};
use crate::server::release_notes::catalog::{CHANGELOG_URL, ReleaseNote};

/// Sent to every user when the server starts on a release marked for email.
/// Carries the notes of every release since the last one emailed, newest first.
pub struct ReleaseNotes<'a> {
    /// Newest first, at most `catalog::MAX_SHOWN`.
    pub notes: &'a [&'a ReleaseNote],
    /// Whether older releases were left out for length.
    pub truncated: bool,
}

impl Email for ReleaseNotes<'_> {
    fn subject(&self) -> String {
        match self.notes.first() {
            Some(newest) => format!("Scanopy {}: {}", newest.version, newest.title),
            None => "What's new in Scanopy".to_string(),
        }
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::ProductUpdates
    }

    fn campaign(&self) -> &'static str {
        "release_notes"
    }

    fn body_html(&self) -> String {
        let mut content = Content::new().heading("What's new in Scanopy");
        for note in self.notes {
            content = content
                .subheading(&format!(
                    "{} <span style=\"font-weight: 400; color: #9ca3af;\">v{}</span>",
                    html_escape(&note.title),
                    note.version
                ))
                .raw(&format!(
                    "<div style=\"margin: 0 0 24px 0; font-size: 16px; line-height: 24px; color: #4a4a4a;\">{}</div>\n",
                    note.html
                ));
        }
        if self.truncated {
            content = content.paragraph(&format!(
                "Earlier releases are in the <a href=\"{CHANGELOG_URL}\">changelog</a>."
            ));
        }
        Body::new()
            .content(content)
            .cta(links::APP_HOME, "Open Scanopy")
            .render()
    }
}
