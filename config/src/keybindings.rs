//! `[keybindings]` overrides in `config.toml`.
//!
//! A command id replaces that command's default keys. An empty string or an
//! empty array removes them. Defaults live in `config/default-keybindings.toml`
//! and stay in the web registry; this module only stores overrides.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// TOML section edited in Settings. Not forced or stripped the way desktop
/// startup replaces `[database]`.
pub const SECTION: &str = "keybindings";

const DEFAULTS_TOML: &str = include_str!("../default-keybindings.toml");

/// Parse or conflict failure. `line` and `column` are 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingFault {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

struct Fault {
    command_id: String,
    message: String,
}

/// One binding stored for the client. Field names match the web registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ParsedBinding {
    Chord { chord: ChordBinding },
    Sequence { keys: [String; 2] },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChordBinding {
    pub key: String,
    #[serde(rename = "mod", skip_serializing_if = "is_false")]
    pub mod_key: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub alt: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Raw override values from `config.toml`. Empty removes the shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeybindingValue {
    One(String),
    Many(Vec<String>),
}

/// Overrides only. Absent command ids keep their defaults.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Keybindings {
    pub overrides: BTreeMap<String, KeybindingValue>,
}

#[derive(Serialize)]
pub struct KeybindingsApi {
    pub overrides: BTreeMap<String, Vec<ParsedBinding>>,
}

impl Keybindings {
    pub fn api(&self) -> KeybindingsApi {
        let overrides = self
            .overrides
            .iter()
            .filter_map(|(id, value)| {
                let texts = strings_from_stored(value).ok()?;
                let bindings = texts_to_bindings(&texts).ok()?;
                Some((id.clone(), bindings))
            })
            .collect();
        KeybindingsApi { overrides }
    }
}

pub fn unknown_command(id: &str) -> String {
    format!("Unknown command id \"{id}\".")
}

pub fn malformed_shortcut(text: &str) -> String {
    format!("Malformed shortcut \"{text}\".")
}

pub fn shortcut_conflict(binding: &str, other_id: &str) -> String {
    format!("Shortcut \"{binding}\" is already used by {other_id}.")
}

pub fn quoted_command_id() -> String {
    "Command ids must be quoted, for example \"palette.open\".".to_string()
}

pub fn section_must_be_table() -> String {
    "The [keybindings] section must be a table.".to_string()
}

/// Settings save checks the file text so the error names a line.
pub fn check_document(text: &str) -> Result<(), KeybindingFault> {
    let doc = match toml_edit::ImDocument::<&str>::parse(text) {
        Ok(doc) => doc,
        Err(_) => return Ok(()),
    };
    let Some(item) = doc.get(SECTION) else {
        return Ok(());
    };
    let Some(table) = item.as_table() else {
        return Err(fault_at(text, item, section_must_be_table()));
    };
    let mut overrides = Vec::new();
    for (key, value) in table.iter() {
        if value.is_table() || value.is_inline_table() {
            return Err(fault_at(text, value, quoted_command_id()));
        }
        let texts = match strings_from_item(value) {
            Ok(texts) => texts,
            Err(shown) => return Err(fault_at(text, value, malformed_shortcut(&shown))),
        };
        let bindings = match texts_to_bindings(&texts) {
            Ok(bindings) => bindings,
            Err(shown) => return Err(fault_at(text, value, malformed_shortcut(&shown))),
        };
        overrides.push((key.to_string(), bindings));
    }
    if let Err(error) = validate_parsed(&overrides) {
        let (line, column) = span_for_command(text, &error.command_id).unwrap_or((1, 1));
        return Err(KeybindingFault {
            line,
            column,
            message: error.message,
        });
    }
    Ok(())
}

/// Same rules as [`check_document`], for a config that has already been parsed.
pub fn validate(bindings: &Keybindings) -> Result<(), String> {
    let mut parsed = Vec::new();
    for (id, value) in &bindings.overrides {
        let texts = strings_from_stored(value).map_err(|shown| malformed_shortcut(&shown))?;
        let bindings = texts_to_bindings(&texts).map_err(|shown| malformed_shortcut(&shown))?;
        parsed.push((id.clone(), bindings));
    }
    validate_parsed(&parsed).map_err(|error| error.message)
}

fn strings_from_stored(value: &KeybindingValue) -> Result<Vec<String>, String> {
    match value {
        KeybindingValue::One(text) => strings_from_text(text),
        KeybindingValue::Many(values) => {
            let mut out = Vec::new();
            for text in values {
                if text.is_empty() {
                    return Err(text.clone());
                }
                out.extend(split_or(text)?);
            }
            Ok(out)
        }
    }
}

fn strings_from_item(item: &toml_edit::Item) -> Result<Vec<String>, String> {
    if let Some(text) = item.as_str() {
        return strings_from_text(text);
    }
    if let Some(array) = item.as_array() {
        let mut out = Vec::new();
        for value in array {
            let Some(text) = value.as_str() else {
                return Err(compact(value));
            };
            if text.is_empty() {
                return Err(text.to_string());
            }
            out.extend(split_or(text)?);
        }
        return Ok(out);
    }
    Err(compact(item))
}

fn strings_from_text(text: &str) -> Result<Vec<String>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    split_or(text)
}

fn split_or(text: &str) -> Result<Vec<String>, String> {
    let parts: Vec<&str> = text.split(" or ").collect();
    if parts.iter().any(|part| part.trim().is_empty()) {
        return Err(text.to_string());
    }
    Ok(parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .collect())
}

fn texts_to_bindings(texts: &[String]) -> Result<Vec<ParsedBinding>, String> {
    texts.iter().map(|text| parse_binding(text)).collect()
}

fn parse_binding(text: &str) -> Result<ParsedBinding, String> {
    let lower = text.to_ascii_lowercase();
    if let Some(index) = lower.find(" then ") {
        let left = text[..index].trim();
        let right = text[index + " then ".len()..].trim();
        if left.is_empty() || right.is_empty() || right.to_ascii_lowercase().contains(" then ") {
            return Err(text.to_string());
        }
        return Ok(ParsedBinding::Sequence {
            keys: [
                parse_sequence_key(left).map_err(|_| text.to_string())?,
                parse_sequence_key(right).map_err(|_| text.to_string())?,
            ],
        });
    }
    parse_chord(text)
}

fn parse_sequence_key(text: &str) -> Result<String, String> {
    if text.len() == 1 {
        let ch = text.chars().next().unwrap();
        if ch.is_ascii_alphanumeric() {
            return Ok(ch.to_ascii_lowercase().to_string());
        }
    }
    Err(text.to_string())
}

fn parse_chord(text: &str) -> Result<ParsedBinding, String> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(text.to_string());
    }
    let key_text = parts[parts.len() - 1];
    let mut mod_key = false;
    let mut shift = false;
    let mut alt = false;
    for modifier in &parts[..parts.len() - 1] {
        match modifier.to_ascii_lowercase().as_str() {
            "mod" if !mod_key => mod_key = true,
            "shift" if !shift => shift = true,
            "alt" | "option" if !alt => alt = true,
            _ => return Err(text.to_string()),
        }
    }
    let key = parse_chord_key(key_text).map_err(|_| text.to_string())?;
    let plain_letter = key.len() == 1 && key.chars().all(|ch| ch.is_ascii_lowercase());
    let shift = if plain_letter && !mod_key && !alt {
        false
    } else {
        shift
    };
    Ok(ParsedBinding::Chord {
        chord: ChordBinding {
            key,
            mod_key,
            shift,
            alt,
        },
    })
}

fn parse_chord_key(text: &str) -> Result<String, String> {
    if text == "?" || text == "[" || text == "]" {
        return Ok(text.to_string());
    }
    if text.len() == 1 {
        let ch = text.chars().next().unwrap();
        if ch.is_ascii_alphanumeric() {
            return Ok(ch.to_ascii_lowercase().to_string());
        }
    }
    let normalized = match text.to_ascii_lowercase().as_str() {
        "esc" | "escape" => "escape",
        "enter" | "return" => "enter",
        "down" | "arrowdown" => "arrowdown",
        "up" | "arrowup" => "arrowup",
        "left" | "arrowleft" => "arrowleft",
        "right" | "arrowright" => "arrowright",
        "space" => "space",
        "tab" => "tab",
        _ => return Err(text.to_string()),
    };
    Ok(normalized.to_string())
}

fn validate_parsed(overrides: &[(String, Vec<ParsedBinding>)]) -> Result<(), Fault> {
    let defaults = default_bindings();
    let override_ids: std::collections::BTreeSet<&str> =
        overrides.iter().map(|(id, _)| id.as_str()).collect();
    for id in &override_ids {
        if !defaults.contains_key(*id) {
            return Err(Fault {
                command_id: (*id).to_string(),
                message: unknown_command(id),
            });
        }
    }

    let mut owners: HashMap<String, String> = HashMap::new();
    for (id, bindings) in &defaults {
        if override_ids.contains(id.as_str()) {
            continue;
        }
        for binding in bindings {
            owners.insert(canonical(binding), id.clone());
        }
    }
    for (id, bindings) in overrides {
        for binding in bindings {
            let key = canonical(binding);
            if let Some(other) = owners.get(&key) {
                return Err(Fault {
                    command_id: id.clone(),
                    message: shortcut_conflict(&display_binding(binding), other),
                });
            }
            owners.insert(key, id.clone());
        }
    }

    let mut effective = defaults;
    for (id, bindings) in overrides {
        effective.insert(id.clone(), bindings.clone());
    }
    leader_conflicts(&effective, &override_ids)
}

fn leader_conflicts(
    effective: &BTreeMap<String, Vec<ParsedBinding>>,
    overrides: &std::collections::BTreeSet<&str>,
) -> Result<(), Fault> {
    let mut leaders = Vec::new();
    for (id, bindings) in effective {
        for binding in bindings {
            if let ParsedBinding::Sequence { keys } = binding {
                leaders.push((keys[0].clone(), id.clone()));
            }
        }
    }
    for (id, bindings) in effective {
        for binding in bindings {
            let ParsedBinding::Chord { chord } = binding else {
                continue;
            };
            if chord.mod_key || chord.alt {
                continue;
            }
            let Some((_, other)) = leaders.iter().find(|(leader, _)| leader == &chord.key) else {
                continue;
            };
            let chord_is_override = overrides.contains(id.as_str());
            let sequence_is_override = overrides.contains(other.as_str());
            if !chord_is_override && !sequence_is_override {
                continue;
            }
            let (blame, used_by) = if chord_is_override {
                (id.clone(), other.as_str())
            } else {
                (other.clone(), id.as_str())
            };
            return Err(Fault {
                command_id: blame,
                message: shortcut_conflict(&display_binding(binding), used_by),
            });
        }
    }
    Ok(())
}

fn default_bindings() -> BTreeMap<String, Vec<ParsedBinding>> {
    let raw: BTreeMap<String, Vec<String>> =
        toml::from_str(DEFAULTS_TOML).expect("default keybindings toml");
    raw.into_iter()
        .map(|(id, texts)| {
            let bindings = texts_to_bindings(&texts).expect("default keybinding");
            (id, bindings)
        })
        .collect()
}

fn canonical(binding: &ParsedBinding) -> String {
    display_binding(binding)
}

fn display_binding(binding: &ParsedBinding) -> String {
    match binding {
        ParsedBinding::Chord { chord } => {
            let mut parts = Vec::new();
            if chord.mod_key {
                parts.push("Mod".to_string());
            }
            if chord.alt {
                parts.push("Alt".to_string());
            }
            if chord.shift {
                parts.push("Shift".to_string());
            }
            parts.push(display_key(&chord.key));
            parts.join("+")
        }
        ParsedBinding::Sequence { keys } => {
            format!("{} then {}", display_key(&keys[0]), display_key(&keys[1]))
        }
    }
}

fn display_key(key: &str) -> String {
    if key.len() == 1 && key.chars().all(|ch| ch.is_ascii_lowercase()) {
        return key.to_ascii_uppercase();
    }
    match key {
        "arrowdown" => "Down",
        "arrowup" => "Up",
        "arrowleft" => "Left",
        "arrowright" => "Right",
        "enter" => "Enter",
        "escape" => "Esc",
        "space" => "Space",
        "tab" => "Tab",
        other => other,
    }
    .to_string()
}

fn compact(item: &impl std::fmt::Display) -> String {
    item.to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fault_at(text: &str, item: &toml_edit::Item, message: String) -> KeybindingFault {
    let (line, column) = item
        .span()
        .map(|span| offset_line_col(text, span.start))
        .unwrap_or((1, 1));
    KeybindingFault {
        line,
        column,
        message,
    }
}

fn span_for_command(text: &str, command_id: &str) -> Option<(u32, u32)> {
    let doc = toml_edit::ImDocument::<&str>::parse(text).ok()?;
    let table = doc.get(SECTION)?.as_table()?;
    let start = table.key(command_id)?.span()?.start;
    Some(offset_line_col(text, start))
}

fn offset_line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut column = 1u32;
    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_section_is_valid() {
        check_document("# keep\n[server]\nport = 5000\n").expect("no section");
    }

    #[test]
    fn override_replaces_and_empty_unbinds() {
        let text = "\
[keybindings]
\"palette.open\" = \"Mod+P\"
\"ticket.create\" = \"\"
\"board.nextCard\" = [\"J\", \"Down\"]
";
        check_document(text).expect("valid overrides");
        let config = crate::settings_file::validate_config_text(text).expect("loads");
        assert_eq!(
            config.keybindings.overrides.get("palette.open"),
            Some(&KeybindingValue::One("Mod+P".into()))
        );
        assert_eq!(
            config.keybindings.overrides.get("ticket.create"),
            Some(&KeybindingValue::One(String::new()))
        );
        let api = config.keybindings.api();
        assert_eq!(api.overrides.get("ticket.create").map(Vec::len), Some(0));
        let palette = &api.overrides["palette.open"][0];
        match palette {
            ParsedBinding::Chord { chord } => {
                assert_eq!(chord.key, "p");
                assert!(chord.mod_key);
            }
            other => panic!("chord, got {other:?}"),
        }
    }

    #[test]
    fn unbinding_frees_the_default_key() {
        let text = "\
[keybindings]
\"palette.open\" = \"\"
\"ticket.create\" = \"Mod+K\"
";
        check_document(text).expect("Mod+K is free after unbind");
    }

    #[test]
    fn unknown_command_id_names_the_line() {
        let text = "\
[keybindings]
\"nope\" = \"J\"
";
        let error = check_document(text).expect_err("unknown");
        assert_eq!(error.line, 2, "{}", error.message);
        assert_eq!(error.message, "Unknown command id \"nope\".");
    }

    #[test]
    fn malformed_key_names_the_line() {
        let text = "\
[keybindings]
\"ticket.create\" = \"Ctrl+K\"
";
        let error = check_document(text).expect_err("malformed");
        assert_eq!(error.line, 2, "{}", error.message);
        assert_eq!(error.message, "Malformed shortcut \"Ctrl+K\".");
    }

    #[test]
    fn integer_value_is_a_malformed_shortcut() {
        let text = "\
[keybindings]
\"ticket.create\" = 1
";
        let error = check_document(text).expect_err("integer");
        assert_eq!(error.line, 2, "{}", error.message);
        assert_eq!(error.message, "Malformed shortcut \"1\".");
    }

    #[test]
    fn conflict_with_a_default_names_the_other_command() {
        let text = "\
[keybindings]
\"ticket.create\" = \"Mod+K\"
";
        let error = check_document(text).expect_err("conflict");
        assert_eq!(error.line, 2, "{}", error.message);
        assert_eq!(
            error.message,
            "Shortcut \"Mod+K\" is already used by palette.open."
        );
    }

    #[test]
    fn conflict_between_two_overrides() {
        let text = "\
[keybindings]
\"ticket.create\" = \"X\"
\"ticket.assign\" = \"x\"
";
        let error = check_document(text).expect_err("conflict");
        assert_eq!(
            error.message,
            "Shortcut \"X\" is already used by ticket.create."
        );
        assert_eq!(error.line, 3, "{}", error.message);
    }

    #[test]
    fn unquoted_dotted_command_id_asks_for_quotes() {
        let text = "\
[keybindings]
palette.open = \"Mod+P\"
";
        let error = check_document(text).expect_err("dotted");
        assert_eq!(
            error.message,
            "Command ids must be quoted, for example \"palette.open\"."
        );
    }

    #[test]
    fn plain_letter_that_leads_a_sequence_conflicts() {
        let text = "\
[keybindings]
\"ticket.create\" = \"G\"
";
        let error = check_document(text).expect_err("leader");
        assert!(
            error.message.contains("already used by"),
            "{}",
            error.message
        );
        assert_eq!(error.line, 2);
    }
}
