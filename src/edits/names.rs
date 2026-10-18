//! What each name becomes, from the rename plan.

use std::collections::{HashMap, HashSet};

use crate::rename::RenamePlan;
use crate::symbols::SymbolKind;

/// The names that share a namespace of their own in PHP. A class, an
/// interface, a trait and an enum can't have the same name; a class and a
/// function can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    ClassLike,
    Function,
    Constant,
}

impl Group {
    pub fn of(kind: SymbolKind) -> Option<Group> {
        match kind {
            SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait | SymbolKind::Enum => {
                Some(Group::ClassLike)
            }
            SymbolKind::Function => Some(Group::Function),
            SymbolKind::Constant => Some(Group::Constant),
            SymbolKind::Namespace => None,
        }
    }

    /// Whether `a` and `b` are the same name. Classes and functions ignore
    /// case; constants don't.
    pub fn same(self, a: &str, b: &str) -> bool {
        match self {
            Group::Constant => a == b,
            _ => a.eq_ignore_ascii_case(b),
        }
    }

    fn key(self, name: &str) -> String {
        let name = name.trim_start_matches('\\');

        match self {
            Group::Constant => name.to_string(),
            _ => name.to_ascii_lowercase(),
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    /// The name as the packages declare it.
    name: String,
    replacement: Option<String>,

    /// Whether the name is changed where it is the whole of a string.
    replace_in_strings: bool,
}

#[derive(Debug, Default)]
pub(crate) struct NameMap {
    class_likes: HashMap<String, Entry>,
    functions: HashMap<String, Entry>,
    constants: HashMap<String, Entry>,
    namespaces: HashMap<String, Entry>,

    /// The names that exist once everything is renamed.
    final_class_likes: HashSet<String>,
    final_functions: HashSet<String>,
    final_constants: HashSet<String>,
}

impl NameMap {
    pub fn from_plan(plan: &RenamePlan) -> Self {
        let mut map = NameMap::default();

        for symbol in &plan.symbols {
            let entry = Entry {
                name: symbol.name.clone(),
                replacement: symbol.replacement.clone(),
                replace_in_strings: symbol.replace_in_strings,
            };

            let final_name = symbol.replacement.as_deref().unwrap_or(&symbol.name);

            match Group::of(symbol.kind) {
                Some(group) => {
                    map.entries_mut(group)
                        .insert(group.key(&symbol.name), entry);
                    map.final_names_mut(group).insert(group.key(final_name));
                }
                None => {
                    map.namespaces
                        .insert(symbol.name.to_ascii_lowercase(), entry);
                }
            }
        }

        // Names that were prefixed on an earlier run lead to the same place as
        // the declarations. A name that is declared as it is takes precedence.
        for alias in &plan.already_prefixed {
            let entry = Entry {
                name: alias.original.clone(),
                replacement: Some(alias.current.clone()),
                replace_in_strings: true,
            };

            match Group::of(alias.kind) {
                Some(group) => {
                    map.entries_mut(group)
                        .entry(group.key(&alias.original))
                        .or_insert(entry);
                }
                None => {
                    map.namespaces
                        .entry(alias.original.to_ascii_lowercase())
                        .or_insert(entry);
                }
            }
        }

        map
    }

    fn entries(&self, group: Group) -> &HashMap<String, Entry> {
        match group {
            Group::ClassLike => &self.class_likes,
            Group::Function => &self.functions,
            Group::Constant => &self.constants,
        }
    }

    fn entries_mut(&mut self, group: Group) -> &mut HashMap<String, Entry> {
        match group {
            Group::ClassLike => &mut self.class_likes,
            Group::Function => &mut self.functions,
            Group::Constant => &mut self.constants,
        }
    }

    fn final_names_mut(&mut self, group: Group) -> &mut HashSet<String> {
        match group {
            Group::ClassLike => &mut self.final_class_likes,
            Group::Function => &mut self.final_functions,
            Group::Constant => &mut self.final_constants,
        }
    }

    /// Whether a symbol of that name was found in the packages.
    pub fn known(&self, group: Group, name: &str) -> bool {
        self.entries(group).contains_key(&group.key(name))
    }

    /// What the symbol is renamed to, if it is.
    pub fn replacement(&self, group: Group, name: &str) -> Option<&str> {
        self.entries(group)
            .get(&group.key(name))
            .and_then(|entry| entry.replacement.as_deref())
    }

    /// What a string that is the whole of a symbol's name becomes, unless the
    /// symbol is one to leave alone in strings.
    pub fn string_replacement(&self, group: Group, name: &str) -> Option<&str> {
        let entry = self.entries(group).get(&group.key(name))?;

        if entry.replace_in_strings {
            entry.replacement.as_deref()
        } else {
            None
        }
    }

    /// What a name with a backslash in it, found in a string or a comment,
    /// becomes: a class, function or constant that was found, a namespace, or
    /// something inside a namespace.
    pub fn new_text_name(&self, name: &str) -> String {
        let name = name.trim_start_matches('\\');

        for group in [Group::ClassLike, Group::Function, Group::Constant] {
            if self.known(group, name) {
                return self.new_name(group, name);
            }
        }

        match self.namespaces.get(&name.to_ascii_lowercase()) {
            Some(entry) => entry
                .replacement
                .clone()
                .unwrap_or_else(|| name.to_string()),
            None => self.through_namespaces(name),
        }
    }

    /// What a namespace written in a string, as in a PSR-4 map, becomes.
    pub fn new_namespace_text(&self, name: &str) -> String {
        let name = name.trim_start_matches('\\');

        match self.namespaces.get(&name.to_ascii_lowercase()) {
            Some(entry) => entry
                .replacement
                .clone()
                .unwrap_or_else(|| name.to_string()),
            None => self.through_namespaces(name),
        }
    }

    /// What a class named in a string becomes.
    pub fn new_class_text(&self, name: &str) -> String {
        self.new_name(Group::ClassLike, name)
    }

    /// Whether a symbol has that name once everything is renamed.
    pub fn exists_after(&self, group: Group, name: &str) -> bool {
        let names = match group {
            Group::ClassLike => &self.final_class_likes,
            Group::Function => &self.final_functions,
            Group::Constant => &self.final_constants,
        };

        names.contains(&group.key(name))
    }

    /// A renamed namespace that `fragment` is the start of, such as `Acme\Core`
    /// for `Acme\`. A namespace that is the whole of `fragment` doesn't count:
    /// a string spelling it is rewritten as it stands.
    pub fn renamed_namespace_starting_with(&self, fragment: &str) -> Option<&str> {
        let fragment = fragment.trim_matches('\\').to_ascii_lowercase();

        // A namespace that is renamed, or inside one that is, is rewritten
        // where it is spelled out.
        if fragment.is_empty() || self.new_namespace_text(&fragment) != fragment {
            return None;
        }

        let start = format!("{fragment}\\");

        self.namespaces
            .iter()
            .filter(|(key, entry)| entry.replacement.is_some() && key.starts_with(&start))
            .map(|(_, entry)| entry.name.as_str())
            .min()
    }

    /// What a declared namespace is called afterwards.
    pub fn namespace(&self, name: &str) -> String {
        match self.namespaces.get(&name.to_ascii_lowercase()) {
            Some(Entry {
                replacement: Some(replacement),
                ..
            }) => replacement.clone(),
            _ => name.to_string(),
        }
    }

    /// What the symbol `name` is called afterwards.
    ///
    /// A symbol that was found is renamed or not as decided. One that wasn't
    /// (a class from a package that couldn't be read, say) is carried along
    /// with the namespace it is in.
    pub fn new_name(&self, group: Group, name: &str) -> String {
        let name = name.trim_start_matches('\\');

        match self.entries(group).get(&group.key(name)) {
            Some(entry) => entry
                .replacement
                .clone()
                .unwrap_or_else(|| name.to_string()),
            None => self.through_namespaces(name),
        }
    }

    /// What the name in `use Name;` becomes. A class import can name a class or
    /// a namespace.
    pub fn new_import_name(&self, group: Group, name: &str) -> String {
        let name = name.trim_start_matches('\\');

        if group == Group::ClassLike && !self.known(group, name) {
            if let Some(entry) = self.namespaces.get(&name.to_ascii_lowercase()) {
                return entry
                    .replacement
                    .clone()
                    .unwrap_or_else(|| name.to_string());
            }
        }

        self.new_name(group, name)
    }

    /// `Ns\Name` becomes `New\Name` if `Ns` is renamed. The longest namespace
    /// that was found decides, so one that isn't renamed holds its contents
    /// back even when a namespace above it is renamed.
    fn through_namespaces(&self, name: &str) -> String {
        let parts: Vec<&str> = name.split('\\').collect();

        for length in (1..parts.len()).rev() {
            let namespace = parts[..length].join("\\");

            if let Some(entry) = self.namespaces.get(&namespace.to_ascii_lowercase()) {
                return match &entry.replacement {
                    Some(replacement) => format!("{replacement}\\{}", parts[length..].join("\\")),
                    None => name.to_string(),
                };
            }
        }

        name.to_string()
    }
}

#[cfg(test)]
impl NameMap {
    /// A map made by hand, for tests: each symbol and what it is renamed to,
    /// if it is.
    pub fn with(mut self, kind: SymbolKind, name: &str, replacement: Option<&str>) -> Self {
        let entry = Entry {
            name: name.to_string(),
            replacement: replacement.map(String::from),
            replace_in_strings: true,
        };

        match Group::of(kind) {
            Some(group) => {
                self.entries_mut(group).insert(group.key(name), entry);
                self.final_names_mut(group)
                    .insert(group.key(replacement.unwrap_or(name)));
            }
            None => {
                self.namespaces.insert(name.to_ascii_lowercase(), entry);
            }
        }

        self
    }

    /// Leaves a symbol alone where it is the whole of a string.
    pub fn without_string_rename(mut self, kind: SymbolKind, name: &str) -> Self {
        let group = Group::of(kind).expect("a namespace is not named in strings");

        self.entries_mut(group)
            .get_mut(&group.key(name))
            .expect("the symbol is in the map")
            .replace_in_strings = false;

        self
    }
}
