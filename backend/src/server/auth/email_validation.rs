//! Signup-time address validation with the Amazon SES v2 `GetEmailAddressInsights` operation.
//!
//! Runs after the local disposable-list and DNS checks in `register`, and only when the
//! `email_validation_aws_*` config is set (Scanopy Cloud). Each call costs $0.01.
//!
//! Policy: reject when SES rates overall validity LOW, or rates the address HIGH for being
//! disposable or randomly generated. Role addresses and MEDIUM validity pass, since they cover
//! real inboxes such as `info@` at a small company. Every failure to get a verdict (timeout,
//! throttling, bad credentials, an unparseable body) lets the signup through (fail open): this
//! is a quality filter, and an SES incident must not become a signup outage.

use std::time::{Duration, SystemTime};

use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use email_address::EmailAddress;
use serde::Deserialize;

use crate::server::config::ServerConfig;
use crate::server::metrics::dependency::{Dependency, DependencyOutcome, observe_http, record};
use crate::server::shared::types::error_codes::ErrorCode;

/// Whole-call timeout. Matches the DNS check so registration never stalls on SES.
const SES_TIMEOUT: Duration = Duration::from_secs(3);

/// SigV4 signing name of the SES v2 API.
const SIGNING_NAME: &str = "ses";

/// A confidence verdict. Unknown strings parse as [`ConfidenceVerdict::Unknown`] and never
/// reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ConfidenceVerdict {
    High,
    Medium,
    Low,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Default, Deserialize)]
struct Verdict {
    #[serde(rename = "ConfidenceVerdict")]
    confidence_verdict: Option<ConfidenceVerdict>,
}

impl Verdict {
    fn is(&self, verdict: ConfidenceVerdict) -> bool {
        self.confidence_verdict == Some(verdict)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Evaluations {
    is_disposable: Verdict,
    is_random_input: Verdict,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct MailboxValidation {
    is_valid: Verdict,
    evaluations: Evaluations,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GetEmailAddressInsightsResponse {
    #[serde(default)]
    mailbox_validation: MailboxValidation,
}

impl MailboxValidation {
    /// The error code to reject the signup with, or `None` to let it through.
    pub fn rejection(&self) -> Option<ErrorCode> {
        if self.evaluations.is_disposable.is(ConfidenceVerdict::High) {
            Some(ErrorCode::ValidationEmailDisposable)
        } else if self.is_valid.is(ConfidenceVerdict::Low)
            || self.evaluations.is_random_input.is(ConfidenceVerdict::High)
        {
            Some(ErrorCode::ValidationEmailUndeliverable)
        } else {
            None
        }
    }
}

/// Client for `GetEmailAddressInsights`. Present only when region and IAM keys are configured.
pub struct SesEmailValidator {
    http: reqwest::Client,
    region: String,
    identity: Identity,
    endpoint: String,
}

impl SesEmailValidator {
    /// `None` unless all three `email_validation_aws_*` settings are present and non-empty.
    pub fn from_config(config: &ServerConfig) -> anyhow::Result<Option<Self>> {
        let non_empty =
            |v: &Option<String>| v.as_deref().filter(|s| !s.is_empty()).map(str::to_owned);
        let (Some(region), Some(key_id), Some(secret)) = (
            non_empty(&config.email_validation_aws_region),
            non_empty(&config.email_validation_aws_access_key_id),
            non_empty(&config.email_validation_aws_secret_access_key),
        ) else {
            return Ok(None);
        };
        let http = reqwest::Client::builder()
            .timeout(SES_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let endpoint =
            format!("https://email.{region}.amazonaws.com/v2/email/email-address-insights/");
        let identity = Credentials::new(key_id, secret, None, None, "scanopy-config").into();
        Ok(Some(Self {
            http,
            region,
            identity,
            endpoint,
        }))
    }

    /// The error code to reject `email` with, or `None` to let the signup through. Every
    /// failure returns `None`. Logs never carry the address.
    pub async fn check(&self, email: &EmailAddress) -> Option<ErrorCode> {
        let body = serde_json::json!({ "EmailAddress": email.as_str() }).to_string();
        let request = match self.signed_request(body) {
            Ok(request) => request,
            Err(e) => {
                tracing::warn!(error = %e, "Could not sign the SES email validation request");
                return None;
            }
        };

        let response = match observe_http(Dependency::Ses, request.send()).await {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!(timeout = e.is_timeout(), "SES email validation call failed");
                return None;
            }
        };
        let status = response.status();
        if !status.is_success() {
            tracing::warn!(
                status = status.as_u16(),
                "SES email validation returned an error"
            );
            return None;
        }

        match response.json::<GetEmailAddressInsightsResponse>().await {
            Ok(insights) => insights.mailbox_validation.rejection(),
            Err(_) => {
                record(Dependency::Ses, DependencyOutcome::InvalidResponse);
                tracing::warn!("SES email validation response could not be parsed");
                None
            }
        }
    }

    fn signed_request(&self, body: String) -> anyhow::Result<reqwest::RequestBuilder> {
        const CONTENT_TYPE: &str = "application/json";
        let params = v4::SigningParams::builder()
            .identity(&self.identity)
            .region(&self.region)
            .name(SIGNING_NAME)
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()?
            .into();
        let signable = SignableRequest::new(
            "POST",
            self.endpoint.as_str(),
            [("content-type", CONTENT_TYPE)].into_iter(),
            SignableBody::Bytes(body.as_bytes()),
        )?;
        let (instructions, _) = sign(signable, &params)?.into_parts();

        let mut request = self
            .http
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE);
        for (name, value) in instructions.headers() {
            request = request.header(name, value);
        }
        Ok(request.body(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(json: &str) -> Option<&'static str> {
        serde_json::from_str::<GetEmailAddressInsightsResponse>(json)
            .expect("response parses")
            .mailbox_validation
            .rejection()
            .map(|code| code.code())
    }

    /// A response shaped like SES's, built from `IsValid` and the two evaluations the policy
    /// reads. The other evaluations are filled in so the parser sees the full shape.
    fn response(is_valid: &str, disposable: &str, random: &str, role: &str) -> String {
        format!(
            r#"{{"MailboxValidation":{{"IsValid":{{"ConfidenceVerdict":"{is_valid}"}},"Evaluations":{{
                "HasValidSyntax":{{"ConfidenceVerdict":"HIGH"}},
                "HasValidDnsRecords":{{"ConfidenceVerdict":"HIGH"}},
                "MailboxExists":{{"ConfidenceVerdict":"MEDIUM"}},
                "IsRoleAddress":{{"ConfidenceVerdict":"{role}"}},
                "IsDisposable":{{"ConfidenceVerdict":"{disposable}"}},
                "IsRandomInput":{{"ConfidenceVerdict":"{random}"}}}}}}}}"#
        )
    }

    #[test]
    fn rejects_low_validity_disposable_and_random() {
        assert_eq!(
            decide(&response("LOW", "LOW", "LOW", "LOW")),
            Some(ErrorCode::ValidationEmailUndeliverable.code())
        );
        assert_eq!(
            decide(&response("MEDIUM", "HIGH", "LOW", "LOW")),
            Some(ErrorCode::ValidationEmailDisposable.code())
        );
        assert_eq!(
            decide(&response("MEDIUM", "LOW", "HIGH", "LOW")),
            Some(ErrorCode::ValidationEmailUndeliverable.code())
        );
    }

    #[test]
    fn disposable_wins_over_undeliverable() {
        assert_eq!(
            decide(&response("LOW", "HIGH", "HIGH", "LOW")),
            Some(ErrorCode::ValidationEmailDisposable.code())
        );
    }

    #[test]
    fn accepts_role_addresses_and_medium_signals() {
        assert_eq!(decide(&response("HIGH", "LOW", "LOW", "LOW")), None);
        assert_eq!(decide(&response("MEDIUM", "LOW", "LOW", "HIGH")), None);
        assert_eq!(decide(&response("MEDIUM", "MEDIUM", "MEDIUM", "LOW")), None);
    }

    #[test]
    fn missing_or_unknown_verdicts_fail_open() {
        assert_eq!(decide("{}"), None);
        assert_eq!(decide(r#"{"MailboxValidation":{}}"#), None);
        assert_eq!(
            decide(&response("SOMETHING_NEW", "LOW", "LOW", "LOW")),
            None
        );
        assert_eq!(
            decide(r#"{"MailboxValidation":{"IsValid":{"ConfidenceVerdict":"HIGH"}}}"#),
            None
        );
    }

    #[test]
    fn unconfigured_without_all_three_settings() {
        let full = ServerConfig {
            email_validation_aws_region: Some("us-east-1".into()),
            email_validation_aws_access_key_id: Some("AKIDEXAMPLE".into()),
            email_validation_aws_secret_access_key: Some("secret".into()),
            ..ServerConfig::default()
        };
        assert!(SesEmailValidator::from_config(&full).unwrap().is_some());

        assert!(
            SesEmailValidator::from_config(&ServerConfig::default())
                .unwrap()
                .is_none()
        );
        for missing in 0..3 {
            for value in [None, Some(String::new())] {
                let mut config = full.clone();
                *match missing {
                    0 => &mut config.email_validation_aws_region,
                    1 => &mut config.email_validation_aws_access_key_id,
                    _ => &mut config.email_validation_aws_secret_access_key,
                } = value;
                assert!(SesEmailValidator::from_config(&config).unwrap().is_none());
            }
        }
    }

    #[test]
    fn signs_the_request_for_ses() {
        let config = ServerConfig {
            email_validation_aws_region: Some("eu-west-1".into()),
            email_validation_aws_access_key_id: Some("AKIDEXAMPLE".into()),
            email_validation_aws_secret_access_key: Some("secret".into()),
            ..ServerConfig::default()
        };
        let validator = SesEmailValidator::from_config(&config).unwrap().unwrap();
        let request = validator
            .signed_request(r#"{"EmailAddress":"a@example.com"}"#.into())
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request.url().host_str(),
            Some("email.eu-west-1.amazonaws.com")
        );
        let auth = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .expect("signed");
        assert!(auth.contains("/eu-west-1/ses/aws4_request"));
        assert!(request.headers().contains_key("x-amz-date"));
    }
}
