//! The terms re-acceptance gate.
//!
//! A user whose `terms_accepted_at` is before [`TERMS_ACCEPTANCE_EFFECTIVE`],
//! or who has none on record, must accept the terms again before the API
//! serves their session (`middleware::terms::terms_gate_middleware`). To gate
//! everyone again after the terms change, move the date forward.
//!
//! The gate applies only where acceptance is collected: servers with billing
//! enabled. Registration and OIDC sign-up ask for it only there, so a
//! self-hosted server has nothing to gate on, and this is a constant rather
//! than server config. Demo orgs are exempt: their one shared account would
//! accept on behalf of every visitor.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};

use crate::server::{billing::types::base::BillingPlan, config::ServerConfig};

/// The effective date of the terms every user must have accepted.
pub const TERMS_ACCEPTANCE_EFFECTIVE: NaiveDate = match NaiveDate::from_ymd_opt(2026, 10, 14) {
    Some(date) => date,
    None => panic!("invalid terms effective date"),
};

/// Whether an acceptance at `accepted_at` covers the terms effective on
/// `effective` (from midnight UTC that day).
fn accepted_since(accepted_at: Option<DateTime<Utc>>, effective: NaiveDate) -> bool {
    accepted_at.is_some_and(|at| at >= effective.and_time(NaiveTime::MIN).and_utc())
}

fn acceptance_required_since(
    config: &ServerConfig,
    plan: Option<&BillingPlan>,
    accepted_at: Option<DateTime<Utc>>,
    effective: NaiveDate,
) -> bool {
    config.billing_enabled()
        && !plan.is_some_and(BillingPlan::is_demo)
        && !accepted_since(accepted_at, effective)
}

/// Whether a user in an org on `plan` must accept the current terms before
/// using the app.
pub fn acceptance_required(
    config: &ServerConfig,
    plan: Option<&BillingPlan>,
    accepted_at: Option<DateTime<Utc>>,
) -> bool {
    acceptance_required_since(config, plan, accepted_at, TERMS_ACCEPTANCE_EFFECTIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::billing::plans::get_free_plan;
    use chrono::Days;

    fn billed() -> ServerConfig {
        ServerConfig {
            enforce_billing_for_testing: true,
            ..ServerConfig::default()
        }
    }

    fn midnight(date: NaiveDate) -> DateTime<Utc> {
        date.and_time(NaiveTime::MIN).and_utc()
    }

    #[test]
    fn missing_or_earlier_acceptance_is_gated_and_current_acceptance_is_not() {
        let config = billed();
        let plan = get_free_plan();
        let effective = TERMS_ACCEPTANCE_EFFECTIVE;
        let required = |at| acceptance_required_since(&config, Some(&plan), at, effective);

        assert!(required(None));
        assert!(required(Some(
            midnight(effective) - chrono::Duration::seconds(1)
        )));
        assert!(!required(Some(midnight(effective))));
        assert!(!required(Some(midnight(effective + Days::new(3)))));
    }

    #[test]
    fn moving_the_effective_date_later_gates_a_user_who_accepted_the_earlier_terms() {
        let config = billed();
        let plan = get_free_plan();
        let accepted = Some(midnight(TERMS_ACCEPTANCE_EFFECTIVE));

        assert!(!acceptance_required(&config, Some(&plan), accepted));
        let later = TERMS_ACCEPTANCE_EFFECTIVE + Days::new(90);
        assert!(acceptance_required_since(
            &config,
            Some(&plan),
            accepted,
            later
        ));
    }

    #[test]
    fn servers_without_billing_and_demo_orgs_are_never_gated() {
        let demo = BillingPlan::Demo(crate::server::billing::types::base::PlanConfig::default());
        assert!(!acceptance_required(
            &ServerConfig::default(),
            Some(&get_free_plan()),
            None
        ));
        assert!(!acceptance_required(&billed(), Some(&demo), None));
    }
}
