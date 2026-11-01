//! Explicit interface messages. Project text and media paths are never translated.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Language {
    #[default]
    English,
    Russian,
    French,
    German,
    Spanish,
    Italian,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn native_name(self) -> &'static str {
        [
            "English",
            "Русский",
            "Français",
            "Deutsch",
            "Español",
            "Italiano",
        ][self as usize]
    }
    pub fn from_index(index: u8) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
    pub fn text(self, key: &str) -> &str {
        if self == Self::English {
            return key;
        }
        catalogs()
            .get(key)
            .map_or(key, |values| values[self as usize - 1].as_str())
    }
    pub fn format(self, key: &str, values: &[(&str, String)]) -> String {
        let template = self.text(key);
        // Resolve only placeholders in the template, never in inserted user data.
        let mut output = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            output.push_str(&rest[..start]);
            let Some(end) = rest[start..].find('}') else {
                output.push_str(&rest[start..]);
                return output;
            };
            let name = &rest[start + 1..start + end];
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
                output.push_str(value);
            } else {
                output.push_str(&rest[start..=start + end]);
            }
            rest = &rest[start + end + 1..];
        }
        output.push_str(rest);
        output
    }
}
fn catalogs() -> &'static BTreeMap<String, [String; 5]> {
    static CATALOG: OnceLock<BTreeMap<String, [String; 5]>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("locales/messages.json"))
            .expect("validated built-in translations")
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Debug)]
pub struct Message {
    key: String,
    values: Vec<(&'static str, String)>,
    diagnostic: bool,
}
impl Message {
    pub fn format(key: &'static str, values: &[(&'static str, String)]) -> Self {
        Self {
            key: key.into(),
            values: values.to_vec(),
            diagnostic: false,
        }
    }
    pub fn render(&self, language: Language) -> String {
        if self.diagnostic && !catalogs().contains_key(&self.key) {
            language.format(
                "Operation failed: {details}",
                &[("details", self.key.clone())],
            )
        } else {
            language.format(&self.key, &self.values)
        }
    }
}
impl From<&'static str> for Message {
    fn from(key: &'static str) -> Self {
        Self {
            key: key.into(),
            values: Vec::new(),
            diagnostic: false,
        }
    }
}
impl From<String> for Message {
    fn from(key: String) -> Self {
        Self {
            key,
            values: Vec::new(),
            diagnostic: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<&str> {
        let mut result: Vec<_> = text
            .split('{')
            .skip(1)
            .map(|p| p.split('}').next().unwrap())
            .collect();
        result.sort_unstable();
        result
    }
    #[test]
    fn all_languages_preserve_message_variables() {
        assert!(catalogs().len() > 120);
        for (key, translations) in catalogs() {
            for translation in translations {
                assert!(!translation.trim().is_empty(), "Missing translation: {key}");
                assert_eq!(
                    placeholders(key),
                    placeholders(translation),
                    "Changed variables: {key}"
                );
            }
        }
    }
    #[test]
    fn language_switch_renders_existing_status_and_preserves_user_data() {
        let message = Message::format(
            "Video exported: {path}",
            &[("path", "C:/Photos/{count}/family.avi".into())],
        );
        let russian = message.render(Language::Russian);
        assert!(russian.starts_with("Видео экспортировано:"));
        assert!(russian.ends_with("C:/Photos/{count}/family.avi"));
        assert!(
            message
                .render(Language::French)
                .starts_with("Vidéo exportée")
        );
        assert_eq!(
            Language::French.text("Unrecognized diagnostic"),
            "Unrecognized diagnostic"
        );
    }
}
