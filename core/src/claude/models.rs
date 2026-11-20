//! How Oxen's models are presented to Claude Desktop.
//!
//! Claude Desktop only lists a model whose name reads as one of Anthropic's, so a model
//! from anyone else is given a name of that shape: `claude-via-<UUID>`. The gateway maps
//! that name back to the real model. Claude shows the model's own name as a label, and is
//! told which Claude model to treat it like.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::gateway::Model;

/// The Claude model a non-Claude model is treated like (context handling, features).
pub const BEHAVES_AS: &str = "claude-sonnet-5";

const ALIAS_PREFIX: &str = "claude-via-";

/// One entry in Claude's `inferenceModels` list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelEntry {
    /// The model name Claude sends to the gateway.
    pub name: String,
    /// What Claude shows instead of `name`.
    pub label: Option<String>,
    pub behaves_as: Option<String>,
}

impl From<&str> for ModelEntry {
    fn from(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
}

impl ModelEntry {
    /// The entry as Claude's config expects it: a bare name, or an object when there is
    /// more to say.
    pub fn to_config(&self) -> Value {
        if self.label.is_none() && self.behaves_as.is_none() {
            return json!(self.name);
        }
        let mut entry = serde_json::Map::new();
        entry.insert("name".into(), json!(self.name));
        if let Some(label) = &self.label {
            entry.insert("labelOverride".into(), json!(label));
        }
        if let Some(behaves_as) = &self.behaves_as {
            entry.insert("behavesAs".into(), json!(behaves_as));
        }
        Value::Object(entry)
    }
}

/// The entry Claude needs in order to offer this model.
pub fn entry_for(model: &Model) -> ModelEntry {
    if is_anthropic(model) {
        return ModelEntry::from(model.id.as_str());
    }
    // Oxen's UUID for the model is the stable handle. Until the gateway provides one, the
    // model's name reversed is used, which the gateway also resolves and which keeps other
    // makers' names out of a name Claude inspects.
    let handle = match model.uuid.as_deref().filter(|uuid| is_uuid(uuid)) {
        Some(uuid) => uuid.to_lowercase(),
        None => model.id.chars().rev().collect(),
    };
    ModelEntry {
        name: format!("{ALIAS_PREFIX}{handle}"),
        label: Some(model.name.clone().unwrap_or_else(|| model.id.clone())),
        behaves_as: Some(BEHAVES_AS.into()),
    }
}

/// The list to write into Claude's config, default first.
///
/// With nothing chosen, every model Oxen offers is listed, Anthropic's own first. With a
/// choice, exactly those are listed in the order chosen; one Oxen no longer offers keeps
/// its plain name.
pub fn entries_for(chosen: &[String], catalog: &[Model]) -> Vec<ModelEntry> {
    if chosen.is_empty() {
        let (anthropic, others): (Vec<&Model>, Vec<&Model>) =
            catalog.iter().partition(|model| is_anthropic(model));
        return anthropic.into_iter().chain(others).map(entry_for).collect();
    }
    chosen
        .iter()
        .map(|id| match catalog.iter().find(|model| &model.id == id) {
            Some(model) => entry_for(model),
            None => ModelEntry::from(id.as_str()),
        })
        .collect()
}

fn is_anthropic(model: &Model) -> bool {
    match model.developer.as_deref() {
        Some(developer) => developer.eq_ignore_ascii_case("anthropic"),
        None => model.id.starts_with("claude-"),
    }
}

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61";

    fn model(id: &str, name: &str, developer: &str, uuid: Option<&str>) -> Model {
        Model {
            id: id.into(),
            name: Some(name.into()),
            developer: Some(developer.into()),
            uuid: uuid.map(String::from),
        }
    }

    fn catalog() -> Vec<Model> {
        vec![
            model("gpt-5-mini", "GPT 5 Mini", "openai", Some(UUID)),
            model(
                "claude-opus-5-5",
                "Claude Opus 5.5",
                "anthropic",
                Some("11111111-2222-3333-4444-555555555555"),
            ),
            model("kimi-k3", "Kimi K3", "moonshot", None),
            model("claude-haiku-5-5", "Claude Haiku 5.5", "anthropic", None),
        ]
    }

    #[test]
    fn anthropics_own_models_keep_their_names() {
        let entry = entry_for(&catalog()[1]);
        assert_eq!(entry, ModelEntry::from("claude-opus-5-5"));
        assert_eq!(entry.to_config(), json!("claude-opus-5-5"));
    }

    #[test]
    fn another_makers_model_is_named_by_its_uuid_and_labelled_with_its_own_name() {
        let entry = entry_for(&catalog()[0]);
        assert_eq!(
            entry.to_config(),
            json!({
                "name": format!("claude-via-{UUID}"),
                "labelOverride": "GPT 5 Mini",
                "behavesAs": "claude-sonnet-5"
            })
        );
    }

    #[test]
    fn until_oxen_gives_a_uuid_the_reversed_name_is_used() {
        // The gateway already resolves a reversed name, so this works before UUIDs ship.
        let entry = entry_for(&catalog()[2]);
        assert_eq!(entry.name, "claude-via-3k-imik");
        assert_eq!(entry.label.as_deref(), Some("Kimi K3"));
    }

    #[test]
    fn something_that_is_not_a_uuid_is_not_trusted_as_one() {
        let odd = model("gpt-5-mini", "GPT 5 Mini", "openai", Some("42"));
        assert_eq!(entry_for(&odd).name, "claude-via-inim-5-tpg");
    }

    #[test]
    fn a_model_with_no_friendly_name_is_labelled_with_its_id() {
        let bare = Model::named("llama-4");
        assert_eq!(entry_for(&bare).label.as_deref(), Some("llama-4"));
    }

    #[test]
    fn a_claude_model_is_recognized_even_without_a_maker() {
        // A plain Anthropic-style list says nothing about who makes each model.
        assert_eq!(
            entry_for(&Model::named("claude-sonnet-5")),
            ModelEntry::from("claude-sonnet-5")
        );
    }

    #[test]
    fn with_nothing_chosen_every_model_is_listed_anthropics_first() {
        let names: Vec<String> = entries_for(&[], &catalog())
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(
            names,
            [
                "claude-opus-5-5".to_string(),
                "claude-haiku-5-5".to_string(),
                format!("claude-via-{UUID}"),
                "claude-via-3k-imik".to_string(),
            ]
        );
    }

    #[test]
    fn a_choice_is_listed_in_the_order_chosen() {
        let chosen = ["kimi-k3".to_string(), "claude-opus-5-5".to_string()];
        let names: Vec<String> = entries_for(&chosen, &catalog())
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["claude-via-3k-imik", "claude-opus-5-5"]);
    }

    #[test]
    fn a_chosen_model_oxen_no_longer_lists_keeps_its_plain_name() {
        let chosen = ["retired-model".to_string()];
        assert_eq!(
            entries_for(&chosen, &catalog()),
            vec![ModelEntry::from("retired-model")]
        );
    }

    #[test]
    fn with_no_models_known_nothing_is_listed() {
        assert!(entries_for(&[], &[]).is_empty());
    }
}
