//! Authored gameplay-package DTOs (decoded candidates only).
//!
//! These mirror `gameplay/authoring/src/authoring/definitions.ts` exactly and
//! reuse [`StoredItemDefinition`] as the item candidate: the same authored DTO
//! shape the project document uses, so one semantic conversion
//! (`project_admission::authored_item_definition`) serves both paths and the
//! package cannot drift from project admission semantics.

use serde::Deserialize;

use crate::stored_project::StoredItemDefinition;

pub const LOADING_BAY_GAMEPLAY_SCHEMA_VERSION: u64 = 1;
pub const LOADING_BAY_GAMEPLAY_DOMAIN: &str = "loading-bay";

/// Downstream quotas enforced before compilation (guide: explicit bounds on
/// every downstream collection).
pub const MAX_AUTHORED_ITEMS: usize = 64;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AuthoredGameplayPayload {
    pub schema_version: u64,
    pub items: Vec<StoredItemDefinition>,
}

/// The schema-2 binary64 wire spells EVERY number as a double (`200.0`), while
/// the shared item DTO's integer fields expect JSON integers. This compiler-
/// owned normalization accepts integral binary64 values and rewrites them to
/// JSON integers (mirroring Dagger's integral-newtype boundary), rejecting
/// non-integral values in integer positions at DTO decode instead. Bounded by
/// the envelope's own payload limits.
pub fn normalize_binary64_integers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                return;
            }
            if let Some(float) = number.as_f64() {
                if float.fract() == 0.0 && float >= i64::MIN as f64 && float <= i64::MAX as f64 {
                    *number = serde_json::Number::from(float as i64);
                }
            }
        }
        serde_json::Value::Array(entries) => {
            for entry in entries.iter_mut() {
                normalize_binary64_integers(entry);
            }
        }
        serde_json::Value::Object(entries) => {
            for entry in entries.values_mut() {
                normalize_binary64_integers(entry);
            }
        }
        _ => {}
    }
}
