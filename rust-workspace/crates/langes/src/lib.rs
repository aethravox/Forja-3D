#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangTable {
    pub current_lang: String,
    entries: Vec<(String, String)>,
}

impl Default for LangTable {
    fn default() -> Self {
        Self {
            current_lang: String::from("es"),
            entries: Vec::new(),
        }
    }
}

impl LangTable {
    pub fn new(lang: impl Into<String>) -> Self {
        Self {
            current_lang: lang.into(),
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let v = value.into();
        if let Some(pos) = self.entries.iter().position(|(ek, _)| ek == &k) {
            self.entries[pos].1 = v;
        } else {
            self.entries.push((k, v));
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn get_or_default<'a>(&'a self, key: &'a str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    /// Parse a `.lang` key-value text document (supports `#` comments, empty lines, and `[lang]` sections).
    pub fn parse_text(source: &str, target_lang: &str) -> Self {
        let mut table = Self::new(target_lang);
        let mut active_section = String::new();

        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                active_section = line[1..line.len() - 1].trim().to_string();
                continue;
            }

            if active_section.is_empty() || active_section == target_lang {
                if let Some((k, v)) = line.split_once('=') {
                    let key = k.trim();
                    let val = v.trim().trim_matches('"').trim_matches('\'');
                    if !key.is_empty() {
                        table.insert(key, val);
                    }
                }
            }
        }
        table
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lang_sections_and_comments() {
        let text = r#"
            # Comentario general
            [es]
            title = "Forja3D Studio"
            save = "Guardar archivo .f3d"

            [en]
            title = "Forja3D Studio"
            save = "Save .f3d file"
        "#;

        let es_table = LangTable::parse_text(text, "es");
        assert_eq!(es_table.get("title"), Some("Forja3D Studio"));
        assert_eq!(es_table.get("save"), Some("Guardar archivo .f3d"));

        let en_table = LangTable::parse_text(text, "en");
        assert_eq!(en_table.get("save"), Some("Save .f3d file"));
    }
}
