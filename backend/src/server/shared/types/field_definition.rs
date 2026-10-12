use anyhow::Error;
use serde::Serialize;
use utoipa::ToSchema;

/// Field types for dynamic form generation.
///
/// The serialized lowercase name is the wire contract with the frontend, which switches on it to
/// pick a renderer and a validator. Adding a variant means teaching the frontend about it.
#[derive(Serialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Single-line text input.
    String,
    /// Multi-line text area.
    Text,
    /// Numeric input with no implied range.
    Number,
    /// Checkbox.
    Boolean,
    /// Fixed choice from `options`.
    Select,
    /// Fixed choice from `options`, every option shown at once. For a short list where seeing the
    /// alternatives is part of choosing, such as the shell an SSH script runs in.
    Radio,
    /// TCP/UDP port. Distinct from `Number` because it carries a 1-65535 range the frontend
    /// validator enforces — and because declaring it is what stops the form guessing "is this a
    /// port?" from the field's label.
    Port,
    /// Secret value supplied either inline or as a path to a file the daemon reads.
    SecretPathOrInline,
    /// Non-secret value supplied either inline or as a path to a file the daemon reads.
    PathOrInline,
    /// An SSH script: a file on the scanned host, a file on the daemon, or text entered here.
    ScriptSource,
}

/// Definition of a form field for dynamic UI rendering.
///
/// One definition serves every dynamic form: credential types and discovery scan settings both
/// build these, and the frontend renders them through the same switch. Reaches the UI through
/// build-time fixtures (`credential-types.json`, `integrations.json`, `scan-settings.json`) and,
/// for the generated TypeScript union, through the OpenAPI schema.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct FieldDefinition {
    /// Server-assigned unique identifier.
    pub id: &'static str,
    /// Human-facing field label.
    pub label: &'static str,
    /// How the field should be rendered and validated.
    pub field_type: FieldType,
    /// Placeholder text for the input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<&'static str>,
    /// Placeholders that replace `placeholder` while another field holds a given value, e.g. a
    /// Windows path once the OS picker says Windows. `field` is a sibling field id, or `daemon_os`
    /// for the credential's own daemon OS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder_by: Option<&'static [DependentPlaceholder]>,
    /// Whether the value is a secret, so it is masked and never echoed back.
    pub secret: bool,
    /// Whether the field may be left empty.
    pub optional: bool,
    /// Explanatory text shown beneath the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_text: Option<&'static str>,
    /// Choices for `Select` fields. Each option carries a wire `value` (the serialized enum
    /// variant) and a human-facing `label`. A `FieldSet::Sets` dropdown takes them from its set's
    /// enum ([`FieldSetValue::options`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<&'static [SelectOption]>,
    /// Value pre-filled when the field is first shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<&'static str>,
    /// For `PathOrInline` and `SecretPathOrInline` fields: what format the inline value must be.
    /// Also drives server-side validation in `CredentialType::validate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_format: Option<InlineFormat>,
    /// Grouping label used to section a long form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<&'static str>,
    /// For a field that can be read from a file on the daemon: an example file name, which the
    /// form joins to the Daemon OS's example directory for the path placeholder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<&'static str>,
    /// Whether the field is short enough to share a row. The form puts two adjacent half-width
    /// fields of the same group side by side.
    pub half_width: bool,
    /// This field's part in a [`FieldSet`]: the dropdown that picks the set, or a field shown only
    /// for some of its values. `None` for a field that always shows.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_field_set"
    )]
    #[schema(value_type = Option<FieldSetRole>)]
    pub field_set: Option<&'static dyn FieldSetBinding>,
}

/// Which fields a form shows, picked by one dropdown.
///
/// `T` is the choice's own enum (e.g. SNMPv3's security level), so the dropdown's values and
/// every `Requires` list are the same type: a field cannot require a value the dropdown does not
/// offer, and one credential type's set cannot name another's values.
#[derive(Debug, Serialize)]
pub enum FieldSet<T: 'static> {
    /// This dropdown picks the set. Its options are `T`'s variants.
    Sets,
    /// Shown only while the dropdown holds one of these values.
    Requires(&'static [T]),
}

/// An enum a [`FieldSet`] dropdown chooses from.
pub trait FieldSetValue:
    Copy + Serialize + std::fmt::Debug + Sync + Into<&'static str> + strum::IntoEnumIterator + 'static
{
    /// Human-facing option label.
    fn label(&self) -> &'static str;

    /// The dropdown's options. Implement as a `LazyLock` over [`field_set_options`]; a static
    /// inside a default method would be one value shared by every implementor.
    fn options() -> &'static [SelectOption];
}

/// One option per variant of `T`, valued with the variant's serialized name.
pub fn field_set_options<T: FieldSetValue>() -> Vec<SelectOption> {
    T::iter()
        .map(|v| SelectOption {
            value: v.into(),
            label: v.label(),
        })
        .collect()
}

/// A [`FieldSet`] with its value type erased, so fields of every credential type share one
/// `FieldDefinition`. Serializes through `T`'s own `Serialize`.
pub trait FieldSetBinding: std::fmt::Debug + Sync {
    /// The role as the frontend reads it: `"Sets"` or `{"Requires": [...]}`.
    fn to_json(&self) -> serde_json::Value;
    /// Every value the set's dropdown offers.
    fn set_options(&self) -> &'static [SelectOption];
}

impl<T: FieldSetValue> FieldSetBinding for FieldSet<T> {
    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a field set serializes")
    }

    fn set_options(&self) -> &'static [SelectOption] {
        T::options()
    }
}

fn serialize_field_set<S: serde::Serializer>(
    field_set: &Option<&'static dyn FieldSetBinding>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    field_set.map(|f| f.to_json()).serialize(serializer)
}

/// Schema of [`FieldDefinition::field_set`] on the wire. Never constructed: the values are the
/// serialized variants of the set's own enum, which the schema cannot name generically.
#[derive(ToSchema)]
#[allow(dead_code)]
pub enum FieldSetRole {
    /// This dropdown picks the set.
    Sets,
    /// Shown only while the dropdown holds one of these values.
    Requires(Vec<String>),
}

/// A placeholder that applies while `depends_on` holds `value`.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct DependentPlaceholder {
    /// Id of the form field this placeholder depends on (or `daemon_os` for the credential's
    /// Daemon OS).
    pub depends_on: &'static str,
    /// Value that field must hold for this placeholder to apply.
    pub value: &'static str,
    /// Placeholder shown while it does.
    pub placeholder: &'static str,
}

/// A single choice for a `Select` field. `value` is the wire value (serialized enum variant, e.g.
/// "Sha256"); `label` is the human-facing display text.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct SelectOption {
    /// Value submitted when this option is chosen.
    pub value: &'static str,
    /// Human-facing option label.
    pub label: &'static str,
}

/// Format hint for inline values in `PathOrInline` and `SecretPathOrInline` fields.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InlineFormat {
    /// Plain text (e.g. SNMP community string, API key)
    Plain,
    /// PEM-encoded private key
    PemPrivateKey,
    /// PEM-encoded certificate (public, non-secret)
    PemCertificate,
    /// SSH private key: OpenSSH format (`ssh-keygen`'s default) or PKCS#8 / RSA / EC PEM.
    SshPrivateKey,
    /// Six bytes written as a MAC address (e.g. a Wake-on-LAN SecureOn password).
    MacAddress,
    /// A Proxmox VE API token ID: `user@realm!tokenname`.
    ProxmoxTokenId,
}

/// PEM block tag — the label between `-----BEGIN` and `-----`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PemTag {
    Certificate,
    PrivateKey,
    RsaPrivateKey,
    EcPrivateKey,
    OpensshPrivateKey,
}

impl PemTag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certificate => "CERTIFICATE",
            Self::PrivateKey => "PRIVATE KEY",
            Self::RsaPrivateKey => "RSA PRIVATE KEY",
            Self::EcPrivateKey => "EC PRIVATE KEY",
            Self::OpensshPrivateKey => "OPENSSH PRIVATE KEY",
        }
    }
}

impl InlineFormat {
    /// PEM tags accepted by this format, or empty for non-PEM formats.
    pub fn allowed_pem_tags(&self) -> &'static [PemTag] {
        match self {
            Self::Plain | Self::MacAddress | Self::ProxmoxTokenId => &[],
            Self::PemCertificate => &[PemTag::Certificate],
            Self::PemPrivateKey => &[
                PemTag::PrivateKey,
                PemTag::RsaPrivateKey,
                PemTag::EcPrivateKey,
            ],
            Self::SshPrivateKey => &[
                PemTag::OpensshPrivateKey,
                PemTag::PrivateKey,
                PemTag::RsaPrivateKey,
                PemTag::EcPrivateKey,
            ],
        }
    }

    /// Validate a resolved value matches the expected format.
    /// Returns Ok(()) for Plain format (no validation needed).
    pub fn validate(&self, value: &str, field_name: &str) -> Result<(), Error> {
        if let Self::MacAddress = self {
            let trimmed = value.trim();
            if !trimmed.is_empty() && trimmed.parse::<mac_address::MacAddress>().is_err() {
                crate::bail_validation!(
                    "{} must be six bytes written as a MAC address, e.g. 01:23:45:67:89:ab",
                    field_name
                );
            }
            return Ok(());
        }
        if let Self::ProxmoxTokenId = self {
            let trimmed = value.trim();
            if !trimmed.is_empty() && !is_proxmox_token_id(trimmed) {
                crate::bail_validation!(
                    "{} must be written as user@realm!tokenname, e.g. scanopy@pve!discovery",
                    field_name
                );
            }
            return Ok(());
        }
        let tags = self.allowed_pem_tags();
        if tags.is_empty() {
            return Ok(());
        }
        validate_pem(value, field_name, tags)
    }
}

/// `user@realm!tokenname`, as Proxmox VE prints it on token creation. The token name follows
/// Proxmox's own rule: a letter, then letters, digits, `.`, `-` or `_`.
fn is_proxmox_token_id(value: &str) -> bool {
    let Some((userid, token)) = value.split_once('!') else {
        return false;
    };
    let Some((user, realm)) = userid.split_once('@') else {
        return false;
    };
    let plain =
        |s: &str| !s.is_empty() && !s.contains(['@', '!']) && !s.contains(char::is_whitespace);
    let mut token_chars = token.chars();
    plain(user)
        && plain(realm)
        && token_chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && token_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Parse PEM and verify at least one entry has a tag in `allowed_tags`.
fn validate_pem(value: &str, field_name: &str, allowed_tags: &[PemTag]) -> Result<(), Error> {
    use crate::server::shared::types::api::ValidationError;

    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let entries = pem::parse_many(trimmed)
        .map_err(|e| ValidationError::new(format!("{} is not valid PEM: {}", field_name, e)))?;
    if entries.is_empty() {
        crate::bail_validation!("{} contains no PEM data", field_name);
    }
    if !entries
        .iter()
        .any(|p| allowed_tags.iter().any(|t| t.as_str() == p.tag()))
    {
        let expected = allowed_tags
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" or ");
        crate::bail_validation!(
            "{} must contain a {} PEM block, found: {}",
            field_name,
            expected,
            entries
                .iter()
                .map(|p| p.tag().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}
