//! `[keybindings]` overrides in `config.toml`.
//!
//! Lines apply from top to bottom on top of the defaults. A line Coppice
//! cannot use is skipped: that command keeps its default keys, even when an
//! earlier line had set a custom key, and the rest of the table still applies.
//! Defaults live in
//! `config/default-keybindings.toml` and stay in the web registry; this module
//! stores only the commands the table changed, plus the warning sentences.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// TOML section edited in Settings. Not forced or stripped the way desktop
/// startup replaces `[database]`.
pub const SECTION: &str = "keybindings";

const DEFAULTS_TOML: &str = include_str!("../default-keybindings.toml");

/// Spec order. When several sequences share a first key, the warning names the
/// first of them in this list that is still in effect.
const COMMAND_ORDER: &[&str] = &[
    "palette.open",
    "help.show",
    "close",
    "board.nextCard",
    "board.previousCard",
    "board.nextColumn",
    "board.previousColumn",
    "board.column.1",
    "board.column.2",
    "board.column.3",
    "board.column.4",
    "board.column.5",
    "board.column.6",
    "board.column.7",
    "board.column.8",
    "board.column.9",
    "nav.boards",
    "nav.agents",
    "nav.chat",
    "nav.knowledge",
    "nav.repositories",
    "nav.plugins",
    "nav.users",
    "nav.tools",
    "nav.settings",
    "drawer.nextSection",
    "drawer.previousSection",
    "ticket.open",
    "ticket.create",
    "ticket.assign",
    "ticket.moveNext",
    "ticket.movePrevious",
    "ticket.comment",
    "review.approvePlan",
    "review.accept",
    "review.merge",
    "review.openCode",
];

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

/// Commands the table successfully changed, plus skip warnings in file order.
///
/// Serialization writes the overrides as a `[keybindings]` table (normalized
/// shortcut text). Deserialization ignores the value: callers re-read the
/// source with [`from_document`] so file order and skipped lines survive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keybindings {
    pub overrides: BTreeMap<String, Vec<ParsedBinding>>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct KeybindingsApi {
    pub overrides: BTreeMap<String, Vec<ParsedBinding>>,
    pub warnings: Vec<String>,
}

impl Keybindings {
    pub fn api(&self) -> KeybindingsApi {
        KeybindingsApi {
            overrides: self.overrides.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

impl Serialize for Keybindings {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let stored: BTreeMap<String, StoredValue> = self
            .overrides
            .iter()
            .map(|(id, bindings)| (id.clone(), StoredValue::from_bindings(bindings)))
            .collect();
        stored.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Keybindings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Figment has already lost table order, and a bad value must not fail
        // the rest of the file. The source text is applied afterwards.
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Keybindings::default())
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum StoredValue {
    One(String),
    Many(Vec<String>),
}

impl StoredValue {
    fn from_bindings(bindings: &[ParsedBinding]) -> Self {
        match bindings {
            [] => Self::One(String::new()),
            [one] => Self::One(display_binding(one)),
            many => Self::Many(many.iter().map(display_binding).collect()),
        }
    }
}

/// Apply `[keybindings]` from one document, top to bottom.
pub fn from_document(text: &str) -> Keybindings {
    from_texts(std::slice::from_ref(&text))
}

/// Apply each document in order. A later file's lines sit on top of the
/// earlier file's effective keys.
pub fn from_texts(texts: &[&str]) -> Keybindings {
    let mut world = World::defaults();
    let mut warnings = Vec::new();
    for text in texts {
        apply_text(text, &mut world, &mut warnings);
    }
    Keybindings {
        overrides: world.changed,
        warnings,
    }
}

/// Read each existing file and apply it in the order given.
pub fn from_files(paths: &[std::path::PathBuf]) -> Keybindings {
    let texts: Vec<String> = paths
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    let borrowed: Vec<&str> = texts.iter().map(String::as_str).collect();
    from_texts(&borrowed)
}

fn command_label(id: &str) -> Option<&'static str> {
    Some(match id {
        "palette.open" => "Open command palette",
        "help.show" => "Show keyboard shortcuts",
        "close" => "Close",
        "board.nextCard" => "Next card",
        "board.previousCard" => "Previous card",
        "board.nextColumn" => "Next column",
        "board.previousColumn" => "Previous column",
        "board.column.1" => "Go to column 1",
        "board.column.2" => "Go to column 2",
        "board.column.3" => "Go to column 3",
        "board.column.4" => "Go to column 4",
        "board.column.5" => "Go to column 5",
        "board.column.6" => "Go to column 6",
        "board.column.7" => "Go to column 7",
        "board.column.8" => "Go to column 8",
        "board.column.9" => "Go to column 9",
        "nav.boards" => "Go to Boards",
        "nav.agents" => "Go to Agents",
        "nav.chat" => "Go to Chat",
        "nav.knowledge" => "Go to Knowledge",
        "nav.repositories" => "Go to Repositories",
        "nav.plugins" => "Go to Plugins",
        "nav.users" => "Go to Users",
        "nav.tools" => "Go to Tools",
        "nav.settings" => "Go to Settings",
        "drawer.nextSection" => "Next section",
        "drawer.previousSection" => "Previous section",
        "ticket.open" => "Open ticket",
        "ticket.create" => "Create ticket",
        "ticket.assign" => "Assign ticket",
        "ticket.moveNext" => "Move to next column",
        "ticket.movePrevious" => "Move to previous column",
        "ticket.comment" => "Post comment",
        "review.approvePlan" => "Approve plan",
        "review.accept" => "Accept…",
        "review.merge" => "Merge…",
        "review.openCode" => "Open code review",
        _ => return None,
    })
}

fn warn_unknown(id: &str) -> String {
    format!("No command called \"{id}\". No shortcut was changed.")
}

fn warn_unreadable(text: &str, action: &str) -> String {
    format!(
        "Couldn't read \"{text}\". Write it like \"Mod+K\" or \"G then B\". {action} keeps its default."
    )
}

fn warn_clash(binding: &str, other: &str, action: &str) -> String {
    format!("\"{binding}\" is already used by {other}. {action} keeps its default.")
}

fn warn_reserved(binding: &str, action: &str) -> String {
    format!("\"{binding}\" belongs to the browser or the system. {action} keeps its default.")
}

fn warn_quotes() -> String {
    "Put each command id in quotes, like \"palette.open\".".to_string()
}

fn warn_section() -> String {
    "Couldn't read [keybindings]. Each line needs a command id and a key. All shortcuts use their defaults.".to_string()
}

struct World {
    defaults: BTreeMap<String, Vec<ParsedBinding>>,
    bindings: BTreeMap<String, Vec<ParsedBinding>>,
    owners: HashMap<String, String>,
    changed: BTreeMap<String, Vec<ParsedBinding>>,
}

impl World {
    fn defaults() -> Self {
        let defaults = default_bindings();
        let mut owners = HashMap::new();
        for (id, list) in &defaults {
            for binding in list {
                owners.insert(canonical(binding), id.clone());
            }
        }
        Self {
            bindings: defaults.clone(),
            defaults,
            owners,
            changed: BTreeMap::new(),
        }
    }

    fn owner_except(&self, key: &str, id: &str) -> Option<String> {
        self.owners
            .get(key)
            .filter(|owner| owner.as_str() != id)
            .cloned()
    }

    fn first_sequence_owner(&self, leader: &str, except: &str) -> Option<String> {
        for id in COMMAND_ORDER {
            if *id == except {
                continue;
            }
            let Some(list) = self.bindings.get(*id) else {
                continue;
            };
            if list
                .iter()
                .any(|binding| sequence_leader(binding) == Some(leader))
            {
                return Some((*id).to_string());
            }
        }
        None
    }

    fn plain_chord_owner(&self, key: &str, except: &str) -> Option<String> {
        for id in COMMAND_ORDER {
            if *id == except {
                continue;
            }
            let Some(list) = self.bindings.get(*id) else {
                continue;
            };
            if list
                .iter()
                .any(|binding| plain_chord_key(binding) == Some(key))
            {
                return Some((*id).to_string());
            }
        }
        None
    }

    fn claim(&mut self, id: &str, list: Vec<ParsedBinding>) {
        if let Some(previous) = self.bindings.get(id) {
            for binding in previous {
                let key = canonical(binding);
                if self.owners.get(&key).map(String::as_str) == Some(id) {
                    self.owners.remove(&key);
                }
            }
        }
        for binding in &list {
            self.owners.insert(canonical(binding), id.to_string());
        }
        self.bindings.insert(id.to_string(), list.clone());
        self.changed.insert(id.to_string(), list);
    }

    /// A skipped line puts this command back on the built-in keys. An earlier
    /// custom key is dropped, and a key another command took from that default
    /// is given back so two commands do not both keep it.
    fn revert_to_default(&mut self, id: &str) {
        self.release_owned(id);
        let defaults = self.defaults.get(id).cloned().unwrap_or_default();
        for binding in &defaults {
            let key = canonical(binding);
            if let Some(other) = self.owners.get(&key).cloned() {
                if other != id {
                    self.remove_binding(&other, &key);
                }
            }
            self.owners.insert(key, id.to_string());
        }
        self.bindings.insert(id.to_string(), defaults);
        self.changed.remove(id);
    }

    fn release_owned(&mut self, id: &str) {
        let Some(previous) = self.bindings.get(id).cloned() else {
            return;
        };
        for binding in previous {
            let key = canonical(&binding);
            if self.owners.get(&key).map(String::as_str) == Some(id) {
                self.owners.remove(&key);
            }
        }
    }

    fn remove_binding(&mut self, id: &str, key: &str) {
        let Some(list) = self.bindings.get_mut(id) else {
            return;
        };
        list.retain(|binding| canonical(binding) != key);
        let remaining = list.clone();
        let back_to_default = self
            .defaults
            .get(id)
            .is_some_and(|defaults| defaults == &remaining);
        if back_to_default {
            self.changed.remove(id);
        } else {
            self.changed.insert(id.to_string(), remaining);
        }
    }
}

fn apply_text(text: &str, world: &mut World, warnings: &mut Vec<String>) {
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return;
    };
    let Some(item) = doc.get(SECTION) else {
        return;
    };
    let Some(entries) = table_entries(item) else {
        // The section sentence says every shortcut uses its defaults, and that
        // it is the only list item.
        *world = World::defaults();
        warnings.clear();
        warnings.push(warn_section());
        return;
    };
    for (key, value) in entries {
        apply_entry(world, &key, &value, warnings);
    }
}

fn table_entries(item: &toml_edit::Item) -> Option<Vec<(String, toml_edit::Item)>> {
    if let Some(table) = item.as_table() {
        return Some(
            table
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        );
    }
    if let Some(table) = item.as_inline_table() {
        return Some(
            table
                .iter()
                .map(|(key, value)| (key.to_string(), toml_edit::Item::Value(value.clone())))
                .collect(),
        );
    }
    None
}

fn apply_entry(world: &mut World, key: &str, value: &toml_edit::Item, warnings: &mut Vec<String>) {
    if value.as_table().is_some() {
        warnings.push(warn_quotes());
        return;
    }
    let Some(action) = command_label(key) else {
        warnings.push(warn_unknown(key));
        return;
    };
    let parsed = match parse_item(value) {
        Ok(list) => dedupe(list),
        Err(text) => {
            warnings.push(warn_unreadable(&text, action));
            world.revert_to_default(key);
            return;
        }
    };
    match problem(&parsed, world, key) {
        Some(Skip::Reserved(binding)) => {
            warnings.push(warn_reserved(&display_binding(&binding), action));
            world.revert_to_default(key);
        }
        Some(Skip::Clash { binding, other }) => {
            let other_label = command_label(&other).unwrap_or(other.as_str());
            warnings.push(warn_clash(&display_binding(&binding), other_label, action));
            world.revert_to_default(key);
        }
        None => world.claim(key, parsed),
    }
}

enum Skip {
    Reserved(ParsedBinding),
    Clash {
        binding: ParsedBinding,
        other: String,
    },
}

fn problem(list: &[ParsedBinding], world: &World, id: &str) -> Option<Skip> {
    for binding in list {
        if is_reserved(binding) {
            return Some(Skip::Reserved(binding.clone()));
        }
    }
    for (index, binding) in list.iter().enumerate() {
        if let Some(other) = world.owner_except(&canonical(binding), id) {
            return Some(Skip::Clash {
                binding: binding.clone(),
                other,
            });
        }
        if let Some(other) = prefix_conflict(binding, world, id, &list[..index]) {
            return Some(Skip::Clash {
                binding: binding.clone(),
                other,
            });
        }
    }
    None
}

fn prefix_conflict(
    binding: &ParsedBinding,
    world: &World,
    id: &str,
    earlier: &[ParsedBinding],
) -> Option<String> {
    match binding {
        ParsedBinding::Chord { chord } if !chord.mod_key && !chord.alt => {
            if let Some(other) = world.first_sequence_owner(&chord.key, id) {
                return Some(other);
            }
            if earlier
                .iter()
                .any(|earlier| sequence_leader(earlier) == Some(chord.key.as_str()))
            {
                return Some(id.to_string());
            }
            None
        }
        ParsedBinding::Sequence { keys } => {
            if let Some(other) = world.plain_chord_owner(&keys[0], id) {
                return Some(other);
            }
            if earlier
                .iter()
                .any(|earlier| plain_chord_key(earlier) == Some(keys[0].as_str()))
            {
                return Some(id.to_string());
            }
            None
        }
        ParsedBinding::Chord { .. } => None,
    }
}

fn sequence_leader(binding: &ParsedBinding) -> Option<&str> {
    match binding {
        ParsedBinding::Sequence { keys } => Some(keys[0].as_str()),
        ParsedBinding::Chord { .. } => None,
    }
}

/// A chord with no Mod and no Alt. Shift on a non-letter still counts: the
/// sequence clash is about the key, and a bare letter has already dropped Shift.
fn plain_chord_key(binding: &ParsedBinding) -> Option<&str> {
    match binding {
        ParsedBinding::Chord { chord } if !chord.mod_key && !chord.alt => Some(chord.key.as_str()),
        _ => None,
    }
}

fn dedupe(list: Vec<ParsedBinding>) -> Vec<ParsedBinding> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for binding in list {
        if seen.insert(canonical(&binding)) {
            out.push(binding);
        }
    }
    out
}

fn parse_item(item: &toml_edit::Item) -> Result<Vec<ParsedBinding>, String> {
    if let Some(text) = item.as_str() {
        return parse_text(text);
    }
    if let Some(array) = item.as_array() {
        if array.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for value in array {
            let Some(text) = value.as_str() else {
                return Err(compact(value));
            };
            match parse_text(text) {
                Ok(list) if list.is_empty() => return Err(text.trim().to_string()),
                Ok(list) => out.extend(list),
                Err(shown) => return Err(shown),
            }
        }
        return Ok(out);
    }
    Err(compact(item))
}

fn parse_text(raw: &str) -> Result<Vec<ParsedBinding>, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = text.split(" or ").collect();
    if parts.iter().any(|part| part.trim().is_empty()) {
        return Err(text.to_string());
    }
    let mut out = Vec::new();
    for part in parts {
        let part = part.trim();
        out.push(parse_binding(part).map_err(|_| part.to_string())?);
    }
    Ok(out)
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

fn is_reserved(binding: &ParsedBinding) -> bool {
    let shown = display_binding(binding);
    if let Some(digit) = shown.strip_prefix("Mod+") {
        if digit.len() == 1 && digit.chars().all(|ch| ('1'..='9').contains(&ch)) {
            return true;
        }
    }
    matches!(
        shown.as_str(),
        "Mod+Q"
            | "Mod+W"
            | "Mod+T"
            | "Mod+N"
            | "Mod+R"
            | "Mod+L"
            | "Mod+Shift+T"
            | "Mod+Shift+W"
            | "Mod+Shift+["
            | "Mod+Shift+]"
            | "Mod+Alt+Left"
            | "Mod+Alt+Right"
            | "Mod+Alt+Up"
            | "Mod+Alt+Down"
            | "Alt+Left"
            | "Alt+Right"
            | "Mod+H"
            | "Mod+M"
            | "Mod+Tab"
            | "Mod+Space"
            | "Mod+C"
            | "Mod+V"
            | "Mod+X"
            | "Mod+A"
            | "Mod+Z"
            | "Mod+Shift+M"
    )
}

/// Every command id with its default keys as written in the defaults file,
/// in [`COMMAND_ORDER`].
pub fn default_commands() -> Vec<(String, Vec<String>)> {
    let mut raw: BTreeMap<String, Vec<String>> =
        toml::from_str(DEFAULTS_TOML).expect("default keybindings toml");
    COMMAND_ORDER
        .iter()
        .filter_map(|id| raw.remove(*id).map(|keys| (id.to_string(), keys)))
        .collect()
}

fn default_bindings() -> BTreeMap<String, Vec<ParsedBinding>> {
    let raw: BTreeMap<String, Vec<String>> =
        toml::from_str(DEFAULTS_TOML).expect("default keybindings toml");
    raw.into_iter()
        .map(|(id, texts)| {
            let mut bindings = Vec::new();
            for text in texts {
                bindings.extend(parse_text(&text).expect("default keybinding"));
            }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn api(text: &str) -> KeybindingsApi {
        from_document(text).api()
    }

    fn mod_key(bindings: &[ParsedBinding], key: &str) -> bool {
        bindings.iter().any(|binding| {
            matches!(
                binding,
                ParsedBinding::Chord { chord }
                    if chord.key == key && chord.mod_key && !chord.alt && !chord.shift
            )
        })
    }

    #[test]
    fn command_labels_match_the_default_file() {
        let defaults = default_bindings();
        let labeled: BTreeSet<&str> = COMMAND_ORDER.iter().copied().collect();
        let file_ids: BTreeSet<&str> = defaults.keys().map(String::as_str).collect();
        assert_eq!(labeled, file_ids);
        for id in COMMAND_ORDER {
            assert!(command_label(id).is_some(), "{id}");
        }
    }

    #[test]
    fn absent_section_keeps_defaults_and_warns_nothing() {
        let shown = api("# keep\n[server]\nport = 5000\n");
        assert!(shown.overrides.is_empty());
        assert!(shown.warnings.is_empty());
    }

    #[test]
    fn override_replaces_and_empty_unbinds() {
        let text = "\
[keybindings]
\"palette.open\" = \"Mod+P\"
\"ticket.create\" = \"\"
\"board.nextCard\" = [\"J\", \"Down\"]
";
        let config = crate::settings_file::validate_config_text(text).expect("loads");
        let shown = config.keybindings.api();
        assert!(mod_key(&shown.overrides["palette.open"], "p"));
        assert_eq!(shown.overrides.get("ticket.create").map(Vec::len), Some(0));
        assert_eq!(shown.overrides["board.nextCard"].len(), 2);
        assert!(shown.warnings.is_empty());
        assert!(!config
            .keybindings
            .api()
            .overrides
            .contains_key("ticket.assign"));
    }

    #[test]
    fn empty_array_unbinds_and_frees_the_key_for_a_later_line() {
        let shown = api("\
[keybindings]
\"palette.open\" = []
\"ticket.create\" = \"Mod+K\"
");
        assert_eq!(shown.overrides.get("palette.open").map(Vec::len), Some(0));
        assert!(mod_key(&shown.overrides["ticket.create"], "k"));
        assert!(shown.warnings.is_empty());
    }

    #[test]
    fn unbinding_frees_the_default_key_for_a_later_line() {
        let shown = api("\
[keybindings]
\"palette.open\" = \"\"
\"ticket.create\" = \"Mod+K\"
");
        assert!(mod_key(&shown.overrides["ticket.create"], "k"));
        assert!(shown.warnings.is_empty());
    }

    #[test]
    fn the_other_order_skips_the_clash_and_still_unbinds() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"Mod+K\"
\"palette.open\" = \"\"
");
        assert!(!shown.overrides.contains_key("ticket.create"));
        assert_eq!(shown.overrides.get("palette.open").map(Vec::len), Some(0));
        assert_eq!(
            shown.warnings,
            vec![
                "\"Mod+K\" is already used by Open command palette. Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_later_skip_puts_the_command_back_on_its_default() {
        // A repeated key in one TOML table is a syntax error. Two layered
        // files can still set the same command. A skipped later line drops
        // the earlier custom key, so the command keeps its real default.
        let shown = from_texts(&[
            "[keybindings]\n\"ticket.create\" = \"X\"\n",
            "\
[keybindings]
\"ticket.create\" = \"Mod+K\"
\"ticket.assign\" = \"X\"
\"help.show\" = \"C\"
",
        ])
        .api();
        assert!(
            !shown.overrides.contains_key("ticket.create"),
            "skipped command is absent so the client keeps the default: {:?}",
            shown.overrides
        );
        assert!(matches!(
            &shown.overrides["ticket.assign"][0],
            ParsedBinding::Chord { chord } if chord.key == "x" && !chord.mod_key
        ));
        assert!(!shown.overrides.contains_key("help.show"));
        assert_eq!(
            shown.warnings,
            vec![
                "\"Mod+K\" is already used by Open command palette. Create ticket keeps its default."
                    .to_string(),
                "\"C\" is already used by Create ticket. Show keyboard shortcuts keeps its default."
                    .to_string(),
            ]
        );

        let unreadable = from_texts(&[
            "[keybindings]\n\"ticket.create\" = \"X\"\n",
            "[keybindings]\n\"ticket.create\" = \"Ctrl+K\"\n",
        ])
        .api();
        assert!(!unreadable.overrides.contains_key("ticket.create"));
        assert_eq!(
            unreadable.warnings,
            vec![
                "Couldn't read \"Ctrl+K\". Write it like \"Mod+K\" or \"G then B\". Create ticket keeps its default."
                    .to_string()
            ]
        );

        let reserved = from_texts(&[
            "[keybindings]\n\"palette.open\" = \"Mod+P\"\n",
            "[keybindings]\n\"palette.open\" = \"Mod+W\"\n",
        ])
        .api();
        assert!(!reserved.overrides.contains_key("palette.open"));
        assert_eq!(
            reserved.warnings,
            vec![
                "\"Mod+W\" belongs to the browser or the system. Open command palette keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn unknown_id_is_skipped_and_a_sibling_still_applies() {
        let shown = api("\
[keybindings]
\"nope\" = \"Ctrl+K\"
\"palette.open\" = \"Mod+P\"
");
        assert!(mod_key(&shown.overrides["palette.open"], "p"));
        assert!(!shown.overrides.contains_key("nope"));
        assert_eq!(
            shown.warnings,
            vec!["No command called \"nope\". No shortcut was changed.".to_string()]
        );
    }

    #[test]
    fn unreadable_value_keeps_the_default_and_names_the_text() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"Ctrl+K\"
\"palette.open\" = \"Mod+P\"
");
        assert!(!shown.overrides.contains_key("ticket.create"));
        assert!(mod_key(&shown.overrides["palette.open"], "p"));
        assert_eq!(
            shown.warnings,
            vec![
                "Couldn't read \"Ctrl+K\". Write it like \"Mod+K\" or \"G then B\". Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn one_bad_list_item_skips_the_whole_value() {
        let shown = api("\
[keybindings]
\"ticket.create\" = [\"X\", \"Ctrl+K\"]
");
        assert!(!shown.overrides.contains_key("ticket.create"));
        assert_eq!(
            shown.warnings,
            vec![
                "Couldn't read \"Ctrl+K\". Write it like \"Mod+K\" or \"G then B\". Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn integer_value_is_a_malformed_shortcut() {
        let shown = api("\
[keybindings]
\"ticket.create\" = 1
");
        assert_eq!(
            shown.warnings,
            vec![
                "Couldn't read \"1\". Write it like \"Mod+K\" or \"G then B\". Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn reserved_chord_is_skipped_and_a_free_sibling_applies() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"Mod+W\"
\"help.show\" = \"Mod+P\"
");
        assert!(!shown.overrides.contains_key("ticket.create"));
        assert!(mod_key(&shown.overrides["help.show"], "p"));
        assert_eq!(
            shown.warnings,
            vec![
                "\"Mod+W\" belongs to the browser or the system. Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_skipped_reserved_line_does_not_free_the_default_key() {
        let shown = api("\
[keybindings]
\"palette.open\" = \"Mod+W\"
\"ticket.create\" = \"Mod+K\"
");
        assert!(!shown.overrides.contains_key("palette.open"));
        assert!(!shown.overrides.contains_key("ticket.create"));
        assert_eq!(
            shown.warnings,
            vec![
                "\"Mod+W\" belongs to the browser or the system. Open command palette keeps its default.".to_string(),
                "\"Mod+K\" is already used by Open command palette. Create ticket keeps its default.".to_string(),
            ]
        );
    }

    #[test]
    fn reserved_forms_cover_option_alt_and_tab_numbers() {
        let shown = api("\
[keybindings]
\"help.show\" = \"Option+Left\"
\"close\" = \"Mod+1\"
\"ticket.open\" = \"Mod+Shift+M\"
");
        assert_eq!(
            shown.warnings,
            vec![
                "\"Alt+Left\" belongs to the browser or the system. Show keyboard shortcuts keeps its default.".to_string(),
                "\"Mod+1\" belongs to the browser or the system. Close keeps its default.".to_string(),
                "\"Mod+Shift+M\" belongs to the browser or the system. Open ticket keeps its default.".to_string(),
            ]
        );
    }

    #[test]
    fn plain_g_clashes_with_the_first_sequence_still_in_effect() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"G\"
");
        assert_eq!(
            shown.warnings,
            vec![
                "\"G\" is already used by Go to column 1. Create ticket keeps its default."
                    .to_string()
            ]
        );

        let mut lines = vec![
            "[keybindings]".to_string(),
            "\"board.column.1\" = \"\"".to_string(),
        ];
        lines.push("\"ticket.create\" = \"G\"".to_string());
        let shown = api(&lines.join("\n"));
        assert_eq!(
            shown.warnings,
            vec![
                "\"G\" is already used by Go to column 2. Create ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn m_and_r_name_the_first_sequence_in_the_tables() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"M\"
\"ticket.assign\" = \"R\"
");
        assert_eq!(
            shown.warnings,
            vec![
                "\"M\" is already used by Move to next column. Create ticket keeps its default."
                    .to_string(),
                "\"R\" is already used by Approve plan. Assign ticket keeps its default."
                    .to_string(),
            ]
        );
    }

    #[test]
    fn a_sequence_clashes_with_a_plain_chord_that_is_still_held() {
        let shown = api("\
[keybindings]
\"nav.boards\" = \"C then X\"
");
        assert_eq!(
            shown.warnings,
            vec![
                "\"C then X\" is already used by Create ticket. Go to Boards keeps its default."
                    .to_string()
            ]
        );

        let shown = api("\
[keybindings]
\"ticket.create\" = \"\"
\"nav.boards\" = \"C then X\"
");
        assert!(shown.warnings.is_empty(), "{:?}", shown.warnings);
        assert!(matches!(
            &shown.overrides["nav.boards"][0],
            ParsedBinding::Sequence { keys } if keys[0] == "c" && keys[1] == "x"
        ));
    }

    #[test]
    fn mod_g_does_not_clash_with_a_sequence() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"Mod+G\"
");
        assert!(shown.warnings.is_empty(), "{:?}", shown.warnings);
        assert!(mod_key(&shown.overrides["ticket.create"], "g"));
    }

    #[test]
    fn shift_on_a_bare_letter_is_the_letter() {
        let shown = api("\
[keybindings]
\"ticket.assign\" = \"shift+c\"
");
        assert_eq!(
            shown.warnings,
            vec![
                "\"C\" is already used by Create ticket. Assign ticket keeps its default."
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_repeated_key_in_one_value_is_kept_once() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"X or x or X\"
");
        assert_eq!(shown.overrides["ticket.create"].len(), 1);
        assert!(shown.warnings.is_empty());
    }

    #[test]
    fn or_and_a_list_are_several_bindings() {
        let shown = api("\
[keybindings]
\"board.nextCard\" = \"J or Down\"
");
        assert_eq!(shown.overrides["board.nextCard"].len(), 2);
        assert_eq!(display_binding(&shown.overrides["board.nextCard"][0]), "J");
        assert_eq!(
            display_binding(&shown.overrides["board.nextCard"][1]),
            "Down"
        );
    }

    #[test]
    fn g_then_b_is_case_insensitive() {
        let shown = api("\
[keybindings]
\"nav.boards\" = \"\"
\"nav.repositories\" = \"g THEN b\"
");
        assert!(shown.warnings.is_empty(), "{:?}", shown.warnings);
        assert!(matches!(
            &shown.overrides["nav.repositories"][0],
            ParsedBinding::Sequence { keys } if keys[0] == "g" && keys[1] == "b"
        ));
    }

    #[test]
    fn keybindings_section_that_is_not_a_table_warns_once() {
        let config = crate::settings_file::validate_config_text(
            "keybindings = \"nope\"\n[server]\nport = 5001\n",
        )
        .expect("the rest of the file loads");
        assert_eq!(config.server.port, 5001);
        let shown = config.keybindings.api();
        assert!(shown.overrides.is_empty());
        assert_eq!(
            shown.warnings,
            vec![
                "Couldn't read [keybindings]. Each line needs a command id and a key. All shortcuts use their defaults."
                    .to_string()
            ]
        );
    }

    #[test]
    fn unquoted_dotted_id_asks_for_quotes_and_keeps_quoted_siblings() {
        let shown = api("\
[keybindings]
palette.open = \"Mod+P\"
\"ticket.create\" = \"X\"
");
        assert!(!shown.overrides.contains_key("palette.open"));
        assert!(matches!(
            &shown.overrides["ticket.create"][0],
            ParsedBinding::Chord { chord } if chord.key == "x"
        ));
        assert_eq!(
            shown.warnings,
            vec!["Put each command id in quotes, like \"palette.open\".".to_string()]
        );
    }

    #[test]
    fn writing_the_same_default_is_still_an_override() {
        let shown = api("\
[keybindings]
\"ticket.create\" = \"C\"
");
        assert!(shown.overrides.contains_key("ticket.create"));
        assert!(shown.warnings.is_empty());
    }

    #[test]
    fn load_file_only_applies_in_file_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "\
[keybindings]
\"palette.open\" = \"\"
\"ticket.create\" = \"Mod+K\"
\"nope\" = \"J\"
",
        )
        .unwrap();
        let config = crate::AppConfig::load_file_only(&path).unwrap();
        let shown = config.keybindings.api();
        assert_eq!(shown.overrides.get("palette.open").map(Vec::len), Some(0));
        assert!(mod_key(&shown.overrides["ticket.create"], "k"));
        assert!(!shown.overrides.contains_key("nope"));
        assert_eq!(
            shown.warnings,
            vec!["No command called \"nope\". No shortcut was changed.".to_string()]
        );
    }

    #[test]
    fn serialized_overrides_keep_quoted_command_ids() {
        let mut config = crate::AppConfig::load_defaults().unwrap();
        config.keybindings = from_document(
            "[keybindings]\n\"palette.open\" = \"Mod+P\"\n\"ticket.create\" = \"\"\n",
        );
        let text = toml::to_string(&config).unwrap();
        assert!(
            text.contains("\"palette.open\""),
            "dotted ids stay quoted:\n{text}"
        );
        assert!(text.contains("Mod+P"), "{text}");
        assert!(text.contains("ticket.create"), "{text}");
        assert!(!text.contains("warnings"), "{text}");
    }

    #[test]
    fn example_and_docs_use_the_locked_copy() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let comment =
            "# A shortcut Coppice can't use is skipped, and that command keeps its default.";
        for relative in ["config.example.toml", "deploy/config/config.example.toml"] {
            let text = std::fs::read_to_string(root.join(relative)).unwrap();
            assert!(
                text.contains(comment),
                "{relative} is missing the comment line"
            );
        }
        let shortcuts =
            std::fs::read_to_string(root.join("website/src/pages/docs/shortcuts.md")).unwrap();
        assert!(shortcuts.contains(
            "If a shortcut can't be read, names an unknown command, uses a key another command already has, or belongs to the browser or the system, that command keeps its default. Settings explains why. The shortcut help and the command palette show the keys that are in effect."
        ));
        let settings =
            std::fs::read_to_string(root.join("website/src/pages/docs/settings.md")).unwrap();
        assert!(settings.contains(
            "A shortcut Coppice can't read, an unknown command id, a key another command already has, or a key that belongs to the browser or the system is skipped. That command keeps its default, and Settings explains why."
        ));
        assert!(
            !settings.contains("are rejected"),
            "settings.md still says keybindings are rejected"
        );
    }
}
