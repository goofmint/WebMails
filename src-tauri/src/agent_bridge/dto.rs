//! The wire shape `report_unread` receives from the agent (design.md
//! §2.2.6): `UnreadReportDto` and its nested `MessageRefDto`, matching
//! `agent/recipes/types.ts`'s `MessageRef` (design.md §2.2.7).
//!
//! Both types deserialize `camelCase` JSON as-is. Neither has a
//! `#[serde(default)]` on any field: every field the agent sends is
//! required, and a missing one is a deserialization error Tauri surfaces
//! to the caller before `report_unread` is even invoked — not a value
//! this module invents. [`super::validate::validate`] is solely
//! responsible for turning a (successfully deserialized) DTO into a
//! [`super::validate::ValidReport`] or rejecting it.

use serde::{Deserialize, Deserializer};

/// Deserializes a nullable field whose key must still be present:
/// `null` becomes `None`, but an omitted key is an error. (A plain
/// `Option<T>` field would silently default a missing key to `None`.)
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A report of a service's unread state, as sent by the agent's
/// `report.ts` (design.md §2.2.6).
///
/// `observed_at` is kept only as informational data (design.md §2.2.6's
/// closing note: "`observedAt` is informational only. Liveness uses the
/// time Rust receives the report") — nothing here validates or acts on
/// it beyond carrying it into [`super::validate::ValidReport`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadReportDto {
    pub service_id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub count: Option<i64>,
    pub messages: Vec<MessageRefDto>,
    pub recipe_id: String,
    pub observed_at: u64,
    pub icon_candidates: Vec<String>,
}

/// One message reference inside a report, matching
/// `agent/recipes/types.ts`'s `MessageRef`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRefDto {
    pub id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub from: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub subject: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub link: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_camel_case_fields() {
        let json = serde_json::json!({
            "serviceId": "gmail-personal",
            "count": 3,
            "messages": [
                { "id": "m1", "from": "a@example.com", "subject": "hi", "link": null }
            ],
            "recipeId": "gmail",
            "observedAt": 1_700_000_000_000_u64,
            "iconCandidates": ["https://mail.example.com/favicon.ico"],
        });

        let dto: UnreadReportDto = serde_json::from_value(json).expect("deserialize");

        assert_eq!(dto.service_id, "gmail-personal");
        assert_eq!(dto.count, Some(3));
        assert_eq!(dto.messages.len(), 1);
        assert_eq!(dto.messages[0].id, "m1");
        assert_eq!(dto.messages[0].from.as_deref(), Some("a@example.com"));
        assert_eq!(dto.messages[0].subject.as_deref(), Some("hi"));
        assert_eq!(dto.messages[0].link, None);
        assert_eq!(dto.recipe_id, "gmail");
        assert_eq!(dto.observed_at, 1_700_000_000_000);
        assert_eq!(
            dto.icon_candidates,
            vec!["https://mail.example.com/favicon.ico"]
        );
    }

    #[test]
    fn deserializes_null_count_and_empty_arrays() {
        let json = serde_json::json!({
            "serviceId": "icloud",
            "count": null,
            "messages": [],
            "recipeId": "icloud",
            "observedAt": 0,
            "iconCandidates": [],
        });

        let dto: UnreadReportDto = serde_json::from_value(json).expect("deserialize");

        assert_eq!(dto.count, None);
        assert!(dto.messages.is_empty());
        assert!(dto.icon_candidates.is_empty());
    }

    #[test]
    fn snake_case_field_name_is_rejected() {
        let json = serde_json::json!({
            "service_id": "gmail-personal",
            "count": null,
            "messages": [],
            "recipeId": "gmail",
            "observedAt": 0,
            "iconCandidates": [],
        });

        let result: Result<UnreadReportDto, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_field_is_rejected_without_a_default() {
        let json = serde_json::json!({
            "serviceId": "gmail-personal",
            "count": null,
            "messages": [],
            "recipeId": "gmail",
            "iconCandidates": [],
        });

        let result: Result<UnreadReportDto, _> = serde_json::from_value(json);
        assert!(result.is_err(), "missing observedAt must not default to 0");
    }

    #[test]
    fn rejects_an_omitted_nullable_key() {
        let json = serde_json::json!({
            "serviceId": "gmail-personal",
            "messages": [],
            "recipeId": "gmail",
            "observedAt": 1_700_000_000_000_u64,
            "iconCandidates": [],
        });
        assert!(serde_json::from_value::<UnreadReportDto>(json).is_err());

        let message = serde_json::json!({ "id": "m1", "from": null, "subject": null });
        assert!(serde_json::from_value::<MessageRefDto>(message).is_err());
    }

    #[test]
    fn accepts_explicit_nulls() {
        let message =
            serde_json::json!({ "id": "m1", "from": null, "subject": null, "link": null });
        let parsed =
            serde_json::from_value::<MessageRefDto>(message).expect("explicit nulls are valid");
        assert_eq!(parsed.from, None);
        assert_eq!(parsed.link, None);
    }
}
