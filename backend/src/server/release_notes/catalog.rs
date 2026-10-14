//! The release notes compiled into this binary.
//!
//! `prepare_release.yml` writes one `vX.Y.Z.md` per release into
//! `backend/release-notes/`, and the file is reviewed in the release PR before
//! the build. Every server carries the notes of its own release and every
//! earlier one, so an air-gapped server shows them without fetching anything.

use std::sync::OnceLock;

use chrono::NaiveDate;
use rust_embed::Embed;
use semver::Version;

/// Releases shown at once, in the modal and in the email. Older ones are behind
/// the changelog link.
pub const MAX_SHOWN: usize = 5;

/// The public changelog, for releases past [`MAX_SHOWN`].
pub const CHANGELOG_URL: &str = "https://scanopy.net/changelog";

/// `backend/release-notes/`: the release notes, and the terms updates under
/// `terms/` (see [`super::terms`]).
#[derive(Embed)]
#[folder = "release-notes/"]
pub(crate) struct EmbeddedNotes;

/// The `key: value` lines between a file's `---` fences.
pub(crate) struct Frontmatter<'a>(&'a str);

impl Frontmatter<'_> {
    pub(crate) fn field(&self, key: &str) -> Option<String> {
        self.0.lines().find_map(|line| {
            let (k, v) = line.split_once(':')?;
            (k.trim() == key).then(|| unquote(v.trim()))
        })
    }

    pub(crate) fn required(&self, key: &str) -> Result<String, String> {
        self.field(key).ok_or(format!("missing `{key}`"))
    }
}

/// Split a file into its YAML-style frontmatter and markdown body.
pub(crate) fn split_frontmatter(source: &str) -> Result<(Frontmatter<'_>, &str), String> {
    let rest = source.strip_prefix("---\n").ok_or("missing frontmatter")?;
    let (frontmatter, body) = rest
        .split_once("\n---\n")
        .ok_or("unterminated frontmatter")?;
    Ok((Frontmatter(frontmatter), body))
}

/// Render a markdown body with raw HTML left out.
pub(crate) fn render_markdown(body: &str) -> String {
    comrak::markdown_to_html(body.trim(), &comrak::Options::default())
}

/// One release's notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNote {
    pub version: Version,
    pub date: NaiveDate,
    pub title: String,
    /// Whether upgrading to this release sends the release email.
    pub email: bool,
    /// Whether this release opens the release notes modal on its own. Notes
    /// from before the flag existed have none and open it.
    pub modal: bool,
    /// The body, rendered from markdown with raw HTML left out.
    pub html: String,
}

impl ReleaseNote {
    /// This release's page on the public changelog.
    pub fn url(&self) -> String {
        format!("{CHANGELOG_URL}/v{}", self.version)
    }

    /// Parse a notes file: YAML-style frontmatter (`version`, `date`, `title`,
    /// `email`, optional `modal`), then the markdown body.
    pub fn parse(source: &str) -> Result<Self, String> {
        let (frontmatter, body) = split_frontmatter(source)?;
        let required = |key: &str| frontmatter.required(key);

        let version = required("version")?;
        let version = Version::parse(version.trim_start_matches('v'))
            .map_err(|e| format!("version `{version}`: {e}"))?;
        let date = required("date")?;
        let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|e| format!("date `{date}`: {e}"))?;
        let email = match required("email")?.as_str() {
            "true" => true,
            "false" => false,
            other => return Err(format!("email `{other}` is not true or false")),
        };
        let modal = match frontmatter.field("modal").as_deref() {
            None | Some("true") => true,
            Some("false") => false,
            Some(other) => return Err(format!("modal `{other}` is not true or false")),
        };

        Ok(Self {
            version,
            date,
            title: required("title")?,
            email,
            modal,
            html: render_markdown(body),
        })
    }
}

/// `vX.Y.Z.md`; anything else in the folder, such as its README, is not a release.
fn is_notes_file(name: &str) -> bool {
    name.starts_with('v') && name.ends_with(".md")
}

/// Undo YAML quoting: a single-quoted scalar doubles a literal apostrophe.
fn unquote(value: &str) -> String {
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        inner.replace("''", "'")
    } else if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        inner.to_string()
    } else {
        value.to_string()
    }
}

/// Every release's notes, newest first.
#[derive(Debug, Default)]
pub struct Catalog {
    notes: Vec<ReleaseNote>,
}

impl Catalog {
    /// The notes compiled into this binary. A file that doesn't parse is logged
    /// and left out; it never stops the server.
    pub fn embedded() -> &'static Catalog {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            Catalog::from_files(EmbeddedNotes::iter().filter_map(|name| {
                if !is_notes_file(&name) {
                    return None;
                }
                let file = EmbeddedNotes::get(&name)?;
                Some((
                    name.to_string(),
                    String::from_utf8_lossy(&file.data).into_owned(),
                ))
            }))
        })
    }

    pub fn from_files(files: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut notes: Vec<ReleaseNote> = files
            .into_iter()
            .filter_map(|(name, source)| match ReleaseNote::parse(&source) {
                Ok(note) => Some(note),
                Err(error) => {
                    tracing::error!(file = %name, %error, "Skipping release notes that don't parse");
                    None
                }
            })
            .collect();
        notes.sort_by(|a, b| b.version.cmp(&a.version));
        Self { notes }
    }

    pub fn get(&self, version: &Version) -> Option<&ReleaseNote> {
        self.notes.iter().find(|note| &note.version == version)
    }

    /// Releases newer than `after` up to and including `up_to`, newest first.
    pub fn between(&self, after: &Version, up_to: &Version) -> Vec<&ReleaseNote> {
        self.notes
            .iter()
            .filter(|note| &note.version > after && &note.version <= up_to)
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn note_file(version: &str, email: bool) -> (String, String) {
        note_file_with(version, &format!("email: {email}"))
    }

    /// A notes file with `extra` frontmatter lines after `version`, `date` and `title`.
    pub(crate) fn note_file_with(version: &str, extra: &str) -> (String, String) {
        (
            format!("v{version}.md"),
            format!(
                "---\nversion: {version}\ndate: 2026-10-11\ntitle: 'It''s {version}'\n{extra}\n---\n\n## Fixes\n\n- Fix a thing <script>alert(1)</script>\n"
            ),
        )
    }

    #[test]
    fn parses_frontmatter_and_renders_the_body_without_raw_html() {
        let (_, source) = note_file("0.18.0", true);
        let note = ReleaseNote::parse(&source).unwrap();
        assert_eq!(note.version, Version::new(0, 18, 0));
        assert_eq!(note.title, "It's 0.18.0");
        assert!(note.email);
        assert!(note.html.contains("<h2>Fixes</h2>"));
        assert!(!note.html.contains("<script>"));
    }

    /// Notes written before the `modal` flag existed open the modal, as they did.
    #[test]
    fn modal_defaults_to_on_when_the_flag_is_absent() {
        let modal = |extra: &str| {
            ReleaseNote::parse(&note_file_with("0.18.0", extra).1).map(|note| note.modal)
        };
        assert_eq!(modal("email: false"), Ok(true));
        assert_eq!(modal("email: false\nmodal: false"), Ok(false));
        assert!(modal("email: false\nmodal: maybe").is_err());
    }

    #[test]
    fn a_malformed_file_is_left_out() {
        let catalog = Catalog::from_files([
            note_file("0.18.0", false),
            ("v0.18.1.md".into(), "no frontmatter".into()),
            (
                "v0.18.2.md".into(),
                "---\nversion: 0.18.2\n---\nbody".into(),
            ),
        ]);
        let versions: Vec<_> = catalog
            .notes
            .iter()
            .map(|n| n.version.to_string())
            .collect();
        assert_eq!(versions, ["0.18.0"]);
    }

    #[test]
    fn between_is_newest_first_and_bounded_on_both_sides() {
        let catalog = Catalog::from_files(
            ["0.17.9", "0.18.0", "0.18.1", "0.18.2", "0.19.0"].map(|v| note_file(v, false)),
        );
        let versions: Vec<_> = catalog
            .between(&Version::new(0, 18, 0), &Version::new(0, 18, 2))
            .iter()
            .map(|n| n.version.to_string())
            .collect();
        assert_eq!(versions, ["0.18.2", "0.18.1"]);
    }

    /// Every file shipped in `backend/release-notes/` parses, so a bad merge
    /// fails CI instead of silently dropping a release from the modal.
    #[test]
    fn every_embedded_file_parses() {
        for name in EmbeddedNotes::iter().filter(|name| is_notes_file(name)) {
            let file = EmbeddedNotes::get(&name).unwrap();
            let source = String::from_utf8_lossy(&file.data);
            if let Err(e) = ReleaseNote::parse(&source) {
                panic!("{name}: {e}");
            }
        }
    }
}
