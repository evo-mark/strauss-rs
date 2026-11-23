//! `strauss-rs explain <symbol>`: what happens to one symbol, and why.

use std::fmt::Write;

use serde::Serialize;

use crate::config::Config;
use crate::rename::{PlannedSymbol, Reason, RenamePlan};
use crate::symbols::{BuiltIns, PresetSymbols, SymbolKind, SymbolTable};

/// How many similar names are offered when nothing matches.
const SUGGESTIONS: usize = 8;

#[derive(Debug, Serialize)]
pub struct Explanation {
    pub query: String,
    pub matches: Vec<Match>,
}

/// One symbol the query names. The same name can be a class and a function.
#[derive(Debug, Serialize)]
pub struct Match {
    pub kind: SymbolKind,
    pub name: String,
    pub packages: Vec<String>,
    pub files: Vec<String>,
    pub autoloaded: Option<bool>,
    pub rename: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// `php` or a preset's name, for a symbol that something else provides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provided_by: Option<String>,
    pub reason: String,
    pub reason_category: &'static str,
    /// What to change to get the other result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub left_alone_in_strings: bool,
}

/// Why nothing matched, and what to try.
#[derive(Debug)]
pub struct NotFound {
    pub message: String,
}

pub fn explain(
    config: &Config,
    table: &SymbolTable,
    plan: &RenamePlan,
    query: &str,
    kind: Option<SymbolKind>,
) -> Result<Explanation, NotFound> {
    let name = query.trim().trim_start_matches('\\');
    let wanted = |symbol_kind: SymbolKind| kind.is_none_or(|kind| kind == symbol_kind);

    let mut matches: Vec<Match> = plan
        .symbols
        .iter()
        .filter(|symbol| wanted(symbol.kind) && names_match(symbol.kind, &symbol.name, name))
        .map(planned_match)
        .collect();

    // Left out of the plan because something else provides them.
    for built_in in &table.built_ins {
        if wanted(built_in.kind) && names_match(built_in.kind, &built_in.name, name) {
            matches.push(provided_match(
                built_in.kind,
                &built_in.name,
                &built_in.provided_by,
                Some((&built_in.package, &built_in.file)),
            ));
        }
    }

    if matches.is_empty() {
        matches.extend(provided_without_declaration(config, name, kind));
    }

    if matches.is_empty() {
        return Err(NotFound {
            message: not_found_message(plan, name, kind),
        });
    }

    Ok(Explanation {
        query: query.to_string(),
        matches,
    })
}

/// Class-likes, functions and namespaces are case-insensitive in PHP;
/// constants are not.
fn names_match(kind: SymbolKind, symbol: &str, name: &str) -> bool {
    match kind {
        SymbolKind::Constant => symbol == name,
        _ => symbol.eq_ignore_ascii_case(name),
    }
}

fn planned_match(symbol: &PlannedSymbol) -> Match {
    let mut files: Vec<String> = Vec::new();

    for source in &symbol.sources {
        if !files.contains(&source.file) {
            files.push(source.file.clone());
        }
    }

    Match {
        kind: symbol.kind,
        name: symbol.name.clone(),
        packages: symbol.packages.clone(),
        files,
        autoloaded: Some(symbol.autoloaded),
        rename: symbol.rename,
        replacement: symbol.replacement.clone(),
        provided_by: None,
        reason: symbol.reason.to_string(),
        reason_category: symbol.reason.category(),
        hint: hint(&symbol.reason),
        left_alone_in_strings: symbol.rename && !symbol.replace_in_strings,
    }
}

fn provided_match(
    kind: SymbolKind,
    name: &str,
    provided_by: &str,
    declared_in: Option<(&str, &str)>,
) -> Match {
    let provider = PresetSymbols::title(provided_by);

    let (packages, files, reason) = match declared_in {
        Some((package, file)) => (
            vec![package.to_string()],
            vec![file.to_string()],
            format!("{provider} provides it, so a package's copy is left alone"),
        ),
        None => (
            Vec::new(),
            Vec::new(),
            format!("no package declares it; {provider} provides it, so uses of it are left alone"),
        ),
    };

    let hint = (provided_by != "php").then(|| {
        format!("to rename it like any other symbol, remove `{provided_by}` from exclude_presets")
    });

    Match {
        kind,
        name: name.to_string(),
        packages,
        files,
        autoloaded: None,
        rename: false,
        replacement: None,
        provided_by: Some(provided_by.to_string()),
        reason,
        reason_category: "provided_by_platform",
        hint,
        left_alone_in_strings: false,
    }
}

/// A name no package declares, but PHP or an enabled preset provides.
fn provided_without_declaration(
    config: &Config,
    name: &str,
    kind: Option<SymbolKind>,
) -> Vec<Match> {
    let mut found = Vec::new();
    let built_ins = BuiltIns::get();
    let allowed = |candidate: SymbolKind| kind.is_none_or(|kind| kind == candidate);

    if allowed(SymbolKind::Class) && built_ins.is_class_like(name) {
        found.push(provided_match(SymbolKind::Class, name, "php", None));
    }

    if allowed(SymbolKind::Function) && built_ins.is_function(name) {
        found.push(provided_match(SymbolKind::Function, name, "php", None));
    }

    for preset in &config.exclude_presets {
        let Some(symbols) = PresetSymbols::get(preset) else {
            continue;
        };

        if allowed(SymbolKind::Class) && symbols.is_class_like(name) {
            found.push(provided_match(SymbolKind::Class, name, preset, None));
        }

        if allowed(SymbolKind::Function) && symbols.is_function(name) {
            found.push(provided_match(SymbolKind::Function, name, preset, None));
        }

        if allowed(SymbolKind::Constant) && symbols.is_constant(name) {
            found.push(provided_match(SymbolKind::Constant, name, preset, None));
        }
    }

    found
}

fn not_found_message(plan: &RenamePlan, name: &str, kind: Option<SymbolKind>) -> String {
    let lower = name.to_ascii_lowercase();
    let last = lower.rsplit('\\').next().unwrap_or(&lower).to_string();

    let mut similar: Vec<&str> = plan
        .symbols
        .iter()
        .filter(|symbol| kind.is_none_or(|kind| kind == symbol.kind))
        .map(|symbol| symbol.name.as_str())
        .filter(|candidate| {
            let candidate = candidate.to_ascii_lowercase();

            candidate.contains(&lower)
                || (!last.is_empty() && candidate.rsplit('\\').next() == Some(last.as_str()))
        })
        .collect();

    similar.sort_unstable();
    similar.dedup();

    let mut message = format!("no package declares `{name}`");

    if !similar.is_empty() {
        message.push_str("; similar names:");

        for candidate in similar.iter().take(SUGGESTIONS) {
            write!(message, "\n  {candidate}").unwrap();
        }

        if similar.len() > SUGGESTIONS {
            write!(message, "\n  and {} more", similar.len() - SUGGESTIONS).unwrap();
        }
    }

    message
}

/// What to change to get the other result, for the reasons that come from
/// settings.
fn hint(reason: &Reason) -> Option<String> {
    let hint = match reason {
        Reason::PackageExcludedFromCopy(package) => {
            format!("remove `{package}` from exclude_from_copy.packages to copy and rename it")
        }
        Reason::PackageExcludedFromPrefix(package) => {
            format!("remove `{package}` from exclude_from_prefix.packages to rename it")
        }
        Reason::PackageNotPrefixed(package) => {
            format!("add `{package}` to packages to rename it")
        }
        Reason::NamespaceExcludedFromPrefix(namespace) => {
            format!("remove `{namespace}` from exclude_from_prefix.namespaces to rename it")
        }
        Reason::FileExcludedFromPrefix { pattern, .. } => {
            format!("change `{pattern}` in exclude_from_prefix.file_patterns to rename it")
        }
        Reason::FunctionExcludedFromPrefix(function) => {
            format!("remove `{function}` from exclude_from_prefix.functions to rename it")
        }
        Reason::ConstantPackageExcluded(package) => {
            format!("remove `{package}` from exclude_constants.packages to rename it")
        }
        Reason::ConstantNamespaceExcluded(namespace) => {
            format!("remove `{namespace}` from exclude_constants.namespaces to rename it")
        }
        Reason::ConstantFileExcluded { pattern, .. } => {
            format!("change `{pattern}` in exclude_constants.file_patterns to rename it")
        }
        Reason::ConstantExcluded => {
            "remove it from exclude_constants.constants to rename it".to_string()
        }
        Reason::NotAutoloaded => "only symbols the package autoloads are renamed; check its \
             `autoload`, set `override_autoload`, or add a namespace_replacement_patterns \
             entry for its namespace"
            .to_string(),
        Reason::NoPrefixConfigured(option) => format!("set `{option}` to rename it"),
        Reason::NoNamespaceReplacement => {
            "set `namespace_prefix` or add a namespace_replacement_patterns entry".to_string()
        }
        _ => return None,
    };

    Some(hint)
}

/// The explanation as text for a person.
pub fn render(explanation: &Explanation) -> String {
    let mut output = String::new();

    for (index, item) in explanation.matches.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }

        writeln!(output, "{} {}", item.kind.as_str(), item.name).unwrap();

        if !item.packages.is_empty() {
            writeln!(output, "  declared by  {}", item.packages.join(", ")).unwrap();
        }

        for file in &item.files {
            writeln!(output, "  in           {file}").unwrap();
        }

        if let Some(autoloaded) = item.autoloaded {
            writeln!(
                output,
                "  autoloaded   {}",
                if autoloaded { "yes" } else { "no" }
            )
            .unwrap();
        }

        match &item.replacement {
            Some(replacement) => writeln!(output, "  renamed to   {replacement}").unwrap(),
            None => writeln!(output, "  renamed      no").unwrap(),
        }

        writeln!(output, "  because      {}", item.reason).unwrap();

        if item.left_alone_in_strings {
            writeln!(
                output,
                "  note         not renamed inside strings (exclude_from_string_rename.functions)"
            )
            .unwrap();
        }

        if let Some(hint) = &item.hint {
            writeln!(output, "  to change    {hint}").unwrap();
        }
    }

    output.trim_end().to_string()
}
