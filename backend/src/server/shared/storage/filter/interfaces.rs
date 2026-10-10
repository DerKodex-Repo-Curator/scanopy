//! Interface-specific filters.
use super::*;

use crate::server::interfaces::r#impl::base::Interface;

impl StorableFilter<Interface> {
    /// Interfaces serving any of `vlan_ids`: those whose `native_vlan_id` is one of them, or whose
    /// JSONB `vlan_ids` array holds one of them. An empty selection matches nothing.
    pub fn serving_vlans(mut self, vlan_ids: &[Uuid]) -> Self {
        if vlan_ids.is_empty() {
            self.conditions.push("FALSE".to_string());
            return self;
        }

        let native_col = self.qualify_column("native_vlan_id");
        let members_col = self.qualify_column("vlan_ids");
        let native_idx = self.values.len() + 1;
        let members_idx = native_idx + 1;
        self.conditions.push(format!(
            "({native_col} = ANY(${native_idx}) OR {members_col} ?| ${members_idx})"
        ));
        self.values.push(SqlValue::UuidArray(vlan_ids.to_vec()));
        // `?|` matches the array's text elements, so the ids bind as strings.
        self.values.push(SqlValue::StringArray(
            vlan_ids.iter().map(Uuid::to_string).collect(),
        ));
        self
    }
}
