//! Server instances per online license key.
//!
//! A self-hosted server's instance is its database: the first container to
//! start creates one [`ServerInstance`] row, and every container and replica
//! on that database sends its id with each check-in. The cloud records one
//! [`LicenseInstance`] per id it hears from, and counts the ones heard from
//! within `LICENSE_SILENCE_THRESHOLD_DAYS` against the organization's limit.

pub mod service;

use std::fmt::Display;
use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::server::organizations::r#impl::base::LICENSE_SILENCE_THRESHOLD_DAYS;
use crate::server::shared::storage::traits::{SqlValue, Storable};

/// This database's instance id, held by self-hosted servers. One row per
/// database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerInstance {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Display for ServerInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "server instance {}", self.id)
    }
}

impl Storable for ServerInstance {
    const HAS_SCD2: bool = false;
    type BaseData = ();

    fn table_name() -> &'static str {
        "server_instance"
    }

    fn new(_base: Self::BaseData) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
        }
    }

    fn get_base(&self) -> Self::BaseData {}

    fn to_params(&self) -> Result<(Vec<&'static str>, Vec<SqlValue>), anyhow::Error> {
        Ok((
            vec!["id", "created_at", "updated_at"],
            vec![
                SqlValue::Uuid(self.id),
                SqlValue::Timestamp(self.created_at),
                SqlValue::Timestamp(self.updated_at),
            ],
        ))
    }

    fn from_row(row: &PgRow) -> Result<Self, anyhow::Error> {
        Ok(Self {
            id: row.get("id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }
}

/// A server instance that has checked in with an organization's online key,
/// as the cloud records it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LicenseInstance {
    #[schema(read_only, required)]
    pub id: Uuid,
    #[schema(read_only, required)]
    pub created_at: DateTime<Utc>,
    #[schema(read_only, required)]
    pub updated_at: DateTime<Utc>,
    #[serde(flatten)]
    pub base: LicenseInstanceBase,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LicenseInstanceBase {
    pub organization_id: Uuid,
    /// The id the server's database holds. `None` on the one row shared by
    /// servers too old to send it.
    pub instance_id: Option<Uuid>,
    /// Version the latest check-in reported, as semver text.
    pub server_version: Option<String>,
    /// Source address the cloud observed on the latest check-in. The server
    /// never sends an address; behind NAT several servers share one.
    #[schema(value_type = Option<String>, example = "203.0.113.7")]
    pub last_seen_ip: Option<IpAddr>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl LicenseInstance {
    /// Whether the instance counts against the limit at `now`: it checked in
    /// within `LICENSE_SILENCE_THRESHOLD_DAYS`, the span after which a server
    /// that stopped checking in counts as gone.
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        now - self.base.last_seen_at < chrono::Duration::days(LICENSE_SILENCE_THRESHOLD_DAYS)
    }
}

/// How many of `instances` count against the limit at `now`.
pub fn active_count(instances: &[LicenseInstance], now: DateTime<Utc>) -> u32 {
    let active = instances.iter().filter(|i| i.is_active_at(now)).count();
    u32::try_from(active).unwrap_or(u32::MAX)
}

impl Display for LicenseInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "license instance {}", self.id)
    }
}

impl Storable for LicenseInstance {
    const HAS_SCD2: bool = false;
    type BaseData = LicenseInstanceBase;

    fn table_name() -> &'static str {
        "license_instances"
    }

    fn new(base: Self::BaseData) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            base,
        }
    }

    fn get_base(&self) -> Self::BaseData {
        self.base.clone()
    }

    fn to_params(&self) -> Result<(Vec<&'static str>, Vec<SqlValue>), anyhow::Error> {
        let Self {
            id,
            created_at,
            updated_at,
            base:
                LicenseInstanceBase {
                    organization_id,
                    instance_id,
                    server_version,
                    last_seen_ip,
                    first_seen_at,
                    last_seen_at,
                },
        } = self.clone();

        Ok((
            vec![
                "id",
                "organization_id",
                "instance_id",
                "server_version",
                "last_seen_ip",
                "first_seen_at",
                "last_seen_at",
                "created_at",
                "updated_at",
            ],
            vec![
                SqlValue::Uuid(id),
                SqlValue::Uuid(organization_id),
                SqlValue::OptionalUuid(instance_id),
                SqlValue::OptionalString(server_version),
                SqlValue::OptionalIpAddr(last_seen_ip),
                SqlValue::Timestamp(first_seen_at),
                SqlValue::Timestamp(last_seen_at),
                SqlValue::Timestamp(created_at),
                SqlValue::Timestamp(updated_at),
            ],
        ))
    }

    fn from_row(row: &PgRow) -> Result<Self, anyhow::Error> {
        Ok(Self {
            id: row.get("id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            base: LicenseInstanceBase {
                organization_id: row.get("organization_id"),
                instance_id: row.get("instance_id"),
                server_version: row.get("server_version"),
                last_seen_ip: row
                    .get::<Option<ipnetwork::IpNetwork>, _>("last_seen_ip")
                    .map(|network| network.ip()),
                first_seen_at: row.get("first_seen_at"),
                last_seen_at: row.get("last_seen_at"),
            },
        })
    }
}
