//! `fraise.toml`: the one file a project author writes.
//!
//! Four tools read four configurations, and three of them want a DSN. This module is the
//! replacement: one document, strict about its own shape, from which the files those tools
//! expect are rendered. Two things follow from that, and both are refusals.
//!
//! **A document is read whole or not at all.** Unknown keys are refused rather than ignored
//! (`deny_unknown_fields` on every table of ours), a `default_environment` naming no
//! environment is refused when the file is read rather than when a deploy needs it, and a
//! `${VAR}` that is not set is refused rather than expanded to nothing. The failure being
//! prevented is not a crash: it is the tool that runs, exits zero, and configures something
//! nobody asked for.
//!
//! **A connection string never enters the file.** An environment names the *variable* that
//! carries its DSN — `database_url_env`, whose value must be an environment variable's name —
//! so a password cannot be committed by writing it here, and `fraise` reads the variable at the
//! moment it runs a tool. The refusal that enforces it prints the key and never the value: a
//! refusal that quoted the DSN would put it in the log the rule exists to keep it out of.
//!
//! The `${VAR}` rule is confiture's, copied rather than invented, because `fraise` renders
//! confiture's YAML: a form accepted here and rejected there is a document that loads and
//! then fails one layer down. Measured against the pinned confiture
//! (`confiture.config._env_vars`, byte-identical in 1.19.0 and 1.23.1):
//!
//! | written | confiture | `fraise` |
//! |---|---|---|
//! | `${A}` with `A` set | expands | expands |
//! | `${A}` with `A` unset | refuses | refuses |
//! | `${lower}`, `${1A}` | refuses, naming the strict form | the same |
//! | `${A:-default}` | refuses; bash defaults are not supported | the same |
//! | `${}` | refuses | the same |
//! | `$A` | left alone: only `${…}` is a reference | the same |
//! | a value that expands into `${…}` | refuses; expansion is single-pass | the same |
//! | `${A} ${B` (unclosed, after a closed one) | **expands and leaves `${B`** | **refuses** |
//!
//! The last row is the one difference, and it is deliberate. Confiture checks for an unclosed
//! `${` only at the first one in the value, so a second can survive into the rendered file; the
//! direction that has to be safe is this one, since a document `fraise` accepts must be one
//! confiture accepts and not the reverse. Nothing here leaves a `${` in a resolved value.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::envelope::Payload;
use crate::exit_table::{ExitTable, Refusal};

/// The file's name.
///
/// `fraise` looks for it in one directory: the one it was told to work in. Searching upwards
/// would mean a verb whose configuration depends on where it was invoked from, and the
/// umbrella's directory is a decision its caller makes explicitly.
pub const FILE: &str = "fraise.toml";

/// Where a `${VAR}` is looked up.
///
/// A trait rather than a call to [`std::env`], so that a test states the environment it means
/// instead of mutating the one its process shares with every other test in the binary.
pub trait Vars {
    /// The value of `name`, or `None` when it is not set.
    fn get(&self, name: &str) -> Option<String>;
}

/// The process's own environment, which is what the binary resolves against.
#[derive(Debug, Clone, Copy)]
pub struct Process;

impl Vars for Process {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

impl Vars for BTreeMap<String, String> {
    fn get(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// Why `fraise` will not act on a configuration.
///
/// One class for all of them — the contract's, resolved through [`Refusal::Configuration`] —
/// because from a caller's side they are one answer: the file is what has to change. What
/// differs is the sentence, and every sentence names the key it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    message: String,
}

impl Problem {
    const fn new(message: String) -> Self {
        Self { message }
    }

    /// What to tell the person or the agent that asked.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The exit this refusal carries, in the umbrella's one taxonomy.
    #[must_use]
    pub fn exit(&self) -> i32 {
        ExitTable::vendored().refusal(Refusal::Configuration).exit()
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// `fraise.toml` as it is written: parsed, and checked for everything that can be checked
/// without an environment.
///
/// It hands out nothing. The only thing to do with one is [`Config::resolve`], which is how a
/// `${VAR}` cannot be read as a value by a caller that forgot to expand it — the expanded
/// document is a [`Loaded`] and there is no other way to reach one.
#[derive(Debug)]
pub struct Config {
    path: PathBuf,
    project: Project,
    environments: BTreeMap<String, Environment>,
    tools: BTreeMap<String, toml::Table>,
}

impl Config {
    /// Read the `fraise.toml` of the project in `directory`.
    ///
    /// # Errors
    ///
    /// If there is no such file, or it is not a document this face can act on.
    pub fn at(directory: &Path) -> Result<Self, Problem> {
        let path = directory.join(FILE);
        match fs::read_to_string(&path) {
            Ok(source) => Self::read(&source, path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(Problem::new(format!(
                "there is no {FILE} in {}, so there is no project here to read",
                directory.display()
            ))),
            Err(error) => {
                Err(Problem::new(format!("{FILE} cannot be read: {}: {error}", path.display())))
            },
        }
    }

    /// Read a document that is already in hand, `path` being what to call it in a refusal.
    ///
    /// # Errors
    ///
    /// If it is not TOML, not this document's shape, or says something this face refuses:
    /// a key nothing reads, an environment whose DSN is written where its variable's name
    /// belongs, or a `default_environment` that names no environment.
    pub fn read(source: &str, path: PathBuf) -> Result<Self, Problem> {
        let Document {
            project,
            environments,
            confiture,
            fraiseql,
            fraisier,
            specql,
        } = toml::from_str(source).map_err(|error| {
            Problem::new(format!("{} is not a {FILE}: {error}", path.display()))
        })?;

        unexpanded("project.name", &project.name)?;
        if project.name.trim().is_empty() {
            return Err(Problem::new("project.name is empty".to_owned()));
        }

        for (name, environment) in &environments {
            if !is_environment_name(name) {
                return Err(Problem::new(format!(
                    "{name:?} is not a name an environment can have: it becomes a file name \
                     (db/environments/{name}.yaml), so it is lowercase letters, digits, `_` and \
                     `-`, starting with a letter or a digit"
                )));
            }
            check_variable_name(
                &format!("environments.{name}.database_url_env"),
                &environment.database_url_env,
            )?;
        }

        if let Some(default) = &project.default_environment {
            unexpanded("project.default_environment", default)?;
            if !environments.contains_key(default) {
                let declared = if environments.is_empty() {
                    "the file declares none".to_owned()
                } else {
                    environments.keys().cloned().collect::<Vec<String>>().join(", ")
                };
                return Err(Problem::new(format!(
                    "project.default_environment is {default:?}, which is not an environment this \
                     file declares: {declared}"
                )));
            }
        }

        // The document is strict — those four table names and no others — and what the loader
        // holds is uniform, so expanding, rendering and later writing a tool's file is one piece
        // of code rather than four.
        let tools = [
            ("confiture", confiture),
            ("fraiseql", fraiseql),
            ("fraisier", fraisier),
            ("specql", specql),
        ]
        .into_iter()
        .filter_map(|(tool, table)| table.map(|table| (tool.to_owned(), table)))
        .collect();

        Ok(Self {
            path,
            project,
            environments,
            tools,
        })
    }

    /// Expand every `${VAR}` in the passthrough tables, by confiture's rule.
    ///
    /// # Errors
    ///
    /// If a reference is written in a form confiture would not expand, or names a variable
    /// that is not set. Both name the setting they are in.
    pub fn resolve(self, vars: &dyn Vars) -> Result<Loaded, Problem> {
        let mut from_env = BTreeMap::new();
        let mut settings = BTreeMap::new();
        for (tool, table) in &self.tools {
            let resolved = expand_table(table, tool, vars, &mut from_env)?;
            settings.insert(tool.clone(), resolved);
        }
        Ok(Loaded {
            path: self.path,
            project: self.project,
            environments: self.environments,
            written: self.tools,
            settings,
            from_env,
        })
    }
}

/// `fraise.toml` with every `${VAR}` expanded: what a verb acts on.
///
/// It keeps the document twice on purpose. [`Loaded::settings`] are the values a tool is
/// handed; [`Loaded::view`] is the document *as written*, which is what a reader is shown, and
/// the difference between the two is exactly the values that came from the environment. Never
/// rendering the resolved half is how `show` redacts without a masking rule to get wrong.
#[derive(Debug)]
pub struct Loaded {
    path: PathBuf,
    project: Project,
    environments: BTreeMap<String, Environment>,
    written: BTreeMap<String, toml::Table>,
    settings: BTreeMap<String, toml::Table>,
    from_env: BTreeMap<String, Vec<String>>,
}

impl Loaded {
    /// The project's own settings.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// The environment of that name, or `None` when the file declares no such one.
    #[must_use]
    pub fn environment(&self, name: &str) -> Option<&Environment> {
        self.environments.get(name)
    }

    /// Every environment the file declares, in name order.
    #[must_use]
    pub const fn environments(&self) -> &BTreeMap<String, Environment> {
        &self.environments
    }

    /// What to hand `tool`, with references expanded, or `None` when the file says nothing
    /// about it.
    #[must_use]
    pub fn settings(&self, tool: &str) -> Option<&toml::Table> {
        self.settings.get(tool)
    }

    /// The document as a reader may see it: as written, plus which variables fed which
    /// setting.
    #[must_use]
    pub fn view(&self) -> View<'_> {
        View {
            path: &self.path,
            project: &self.project,
            environments: &self.environments,
            tools: &self.written,
            from_env: &self.from_env,
        }
    }

    /// The view as the envelope's payload: a document, because `fraise` produced it itself.
    ///
    /// # Panics
    ///
    /// If the view cannot be serialised, which is a bug in this module rather than a state a
    /// machine can be in.
    #[must_use]
    pub fn payload(&self) -> Payload {
        Payload::Json(serde_json::to_value(self.view()).expect("a view serialises"))
    }

    /// The view as a person reads it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "{} — {}", self.project.name, self.path.display());
        if let Some(default) = &self.project.default_environment {
            let _ = writeln!(text, "default environment: {default}");
        }

        if !self.environments.is_empty() {
            text.push_str("\nenvironments\n");
            for (name, environment) in &self.environments {
                let _ = writeln!(text, "  {name}  {}", environment.database_url_env);
            }
        }

        for (tool, table) in &self.written {
            let _ = writeln!(text, "\n[{tool}]");
            for (path, value) in flattened(table, tool) {
                let _ = writeln!(text, "  {path}  {value}");
            }
        }

        if !self.from_env.is_empty() {
            text.push_str("\nfrom the environment, and not shown here\n");
            for (path, variables) in &self.from_env {
                let _ = writeln!(text, "  {path}  {}", variables.join(", "));
            }
        }
        text
    }
}

/// The document as `show` renders it, for a person and for a machine alike.
///
/// The tables are the ones the file holds, so a value that came from the environment is here
/// as the `${VAR}` reference that produced it, and `from_env` says which variable fed which
/// setting — which is what an agent needs in order to act, and is not the secret.
#[derive(Debug, Serialize)]
pub struct View<'a> {
    path: &'a Path,
    project: &'a Project,
    environments: &'a BTreeMap<String, Environment>,
    tools: &'a BTreeMap<String, toml::Table>,
    from_env: &'a BTreeMap<String, Vec<String>>,
}

/// `[project]`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    name: String,
    /// The environment a verb uses when it is given none. Checked at load against the
    /// environments the same file declares.
    #[serde(default)]
    default_environment: Option<String>,
}

impl Project {
    /// The project's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The environment to use when a verb is given none, or `None` when the file names one for
    /// every invocation to be explicit about.
    #[must_use]
    pub fn default_environment(&self) -> Option<&str> {
        self.default_environment.as_deref()
    }
}

/// `[environments.<name>]`: one database this project is deployed to.
///
/// One field, and it is a *name*. The DSN itself never appears in `fraise.toml`, which is a
/// committed file; it lives in the environment variable this names, and `fraise` reads it when
/// it runs a tool.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    database_url_env: String,
}

impl Environment {
    /// The name of the environment variable that carries this environment's DSN.
    #[must_use]
    pub fn database_url_env(&self) -> &str {
        &self.database_url_env
    }
}

/// The document as it is written. Every table of ours denies unknown fields: this file is the
/// one a person writes by hand, so a misspelled key is a mistake, and a tolerated one would be
/// a setting that silently does nothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    project: Project,
    #[serde(default)]
    environments: BTreeMap<String, Environment>,
    #[serde(default)]
    confiture: Option<toml::Table>,
    #[serde(default)]
    fraiseql: Option<toml::Table>,
    #[serde(default)]
    fraisier: Option<toml::Table>,
    #[serde(default)]
    specql: Option<toml::Table>,
}

/// Whether `text` is the name of an environment variable, in the strict form confiture's
/// `${VAR}` accepts: `[A-Z_][A-Z0-9_]*`.
fn is_variable_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase() || first == '_')
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Whether `text` is a plain identifier, which is the only shape of a rejected value that is
/// safe to quote back: anything holding a `:`, a `/` or a `@` may be a connection string.
fn is_plain_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Whether `text` can name an environment, which is also a file name in `db/environments/`.
fn is_environment_name(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && text.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        })
}

/// Refuse a field `fraise` reads itself that holds a reference.
///
/// The face's own fields — a project's name, the environment to default to — are resolved
/// against this document and not against the machine, so a reference in one is refused rather
/// than expanded: a file whose own names cannot be read without an environment is a file
/// nobody can read.
fn unexpanded(at: &str, value: &str) -> Result<(), Problem> {
    if value.contains("${") {
        return Err(Problem::new(format!(
            "{at} holds a ${{…}} reference. fraise reads this field itself rather than expanding \
             it, so write the value"
        )));
    }
    Ok(())
}

/// Hold a field that must be an environment variable's name to being one.
///
/// This is the rule that keeps a DSN out of `fraise.toml`, so its refusal is careful about what
/// it prints: the key always, the value only when it cannot be a connection string.
fn check_variable_name(at: &str, value: &str) -> Result<(), Problem> {
    if is_variable_name(value) {
        return Ok(());
    }
    if value.contains("${") {
        return Err(Problem::new(format!(
            "{at} holds a ${{…}} reference where the name of an environment variable belongs. \
             fraise reads the variable itself, at the moment it runs a tool, so write its name"
        )));
    }
    if value.contains("://") || value.contains('@') || value.contains(' ') {
        return Err(Problem::new(format!(
            "{at} holds a connection string, not the name of an environment variable — and it is \
             not quoted back here, because {FILE} is committed and a DSN in a refusal is a DSN in \
             a log. Name the variable that carries it, like MY_APP_DATABASE_URL, and set that \
             variable where the command runs"
        )));
    }
    let quoted = if is_plain_identifier(value) {
        format!(" ({value:?}, did you mean {:?}?)", value.to_ascii_uppercase())
    } else {
        String::new()
    };
    Err(Problem::new(format!(
        "{at} is not the name of an environment variable{quoted}: a name is [A-Z_][A-Z0-9_]*, \
         which is the form confiture expands and fraise looks up"
    )))
}

/// Expand every string in `table`, recording where the environment was read.
fn expand_table(
    table: &toml::Table,
    at: &str,
    vars: &dyn Vars,
    from_env: &mut BTreeMap<String, Vec<String>>,
) -> Result<toml::Table, Problem> {
    let mut expanded = toml::Table::new();
    for (key, value) in table {
        expanded.insert(key.clone(), expand_value(value, &format!("{at}.{key}"), vars, from_env)?);
    }
    Ok(expanded)
}

/// One value, expanded. Tables and arrays are walked so that a reference is found wherever a
/// tool's own configuration puts one, and the path a refusal names is the path a reader can
/// find in the file.
fn expand_value(
    value: &toml::Value,
    at: &str,
    vars: &dyn Vars,
    from_env: &mut BTreeMap<String, Vec<String>>,
) -> Result<toml::Value, Problem> {
    match value {
        toml::Value::String(text) => {
            let (resolved, used) = expanded_string(text, at, vars)?;
            if !used.is_empty() {
                from_env.insert(at.to_owned(), used);
            }
            Ok(toml::Value::String(resolved))
        },
        toml::Value::Table(table) => {
            Ok(toml::Value::Table(expand_table(table, at, vars, from_env)?))
        },
        toml::Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| expand_value(value, &format!("{at}[{index}]"), vars, from_env))
            .collect::<Result<Vec<toml::Value>, Problem>>()
            .map(toml::Value::Array),
        other => Ok(other.clone()),
    }
}

/// One string, expanded by confiture's rule, with the variables it read.
///
/// The scan is confiture's: anything shaped like a reference is found first and then held to
/// the strict form, so a near-miss is refused instead of being left in the value for a tool to
/// receive verbatim. Single-pass, so a value that itself holds a reference is refused rather
/// than expanded again — and refused without being quoted, since what a variable holds is the
/// thing this module is careful with.
fn expanded_string(
    value: &str,
    at: &str,
    vars: &dyn Vars,
) -> Result<(String, Vec<String>), Problem> {
    let mut resolved = String::with_capacity(value.len());
    let mut used = Vec::new();
    let mut rest = value;

    while let Some(open) = rest.find("${") {
        resolved.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find('}') else {
            return Err(Problem::new(format!(
                "{at} holds a `${{` with no closing `}}`, so what it refers to cannot be read"
            )));
        };
        let name = &after[..close];
        if !is_variable_name(name) {
            return Err(Problem::new(diagnosis(name, at)));
        }
        let Some(found) = vars.get(name) else {
            return Err(Problem::new(format!(
                "{at} refers to ${{{name}}}, which is not set. A missing variable is refused \
                 rather than expanded to nothing, so a tool is never handed an empty setting"
            )));
        };
        if found.contains("${") {
            return Err(Problem::new(format!(
                "{at} refers to ${{{name}}}, whose value holds a reference of its own. Expansion \
                 is single-pass here as it is in confiture: resolve the nesting in the \
                 environment rather than in {FILE}"
            )));
        }
        used.push(name.to_owned());
        resolved.push_str(&found);
        rest = &after[close + 1..];
    }
    resolved.push_str(rest);
    Ok((resolved, used))
}

/// Why a reference that is not the strict form is refused, in the terms confiture refuses it.
fn diagnosis(name: &str, at: &str) -> String {
    if name.is_empty() {
        return format!(
            "{at} holds an empty reference `${{}}`: write `${{A_NAME}}` with the name of a \
             variable in it"
        );
    }
    if [":-", ":=", ":?", ":+"].iter().any(|shell| name.contains(shell)) {
        return format!(
            "{at} holds ${{{name}}}: a shell default is not expanded, here or in confiture. \
             Write ${{{}}} and set it, or write the value in {FILE}",
            name.split(':').next().unwrap_or_default()
        );
    }
    let hint = if is_plain_identifier(name) {
        format!(" Did you mean ${{{}}}?", name.to_ascii_uppercase())
    } else {
        String::new()
    };
    format!(
        "{at} holds ${{{name}}}, which is not the name of an environment variable: only \
         [A-Z_][A-Z0-9_]* is expanded — uppercase letters, digits and underscores, the first not \
         a digit — which is confiture's rule and so this file's.{hint}"
    )
}

/// A table's leaves as dotted paths, in the same spelling a refusal and `from_env` use, so a
/// reader matches the three by eye.
fn flattened(table: &toml::Table, at: &str) -> Vec<(String, String)> {
    let mut leaves = Vec::new();
    for (key, value) in table {
        collect_leaves(value, &format!("{at}.{key}"), &mut leaves);
    }
    // The tool's own name is the heading the renderer already printed.
    let prefix = at.len() + 1;
    leaves
        .into_iter()
        .map(|(path, value)| (path[prefix..].to_owned(), value))
        .collect()
}

fn collect_leaves(value: &toml::Value, at: &str, into: &mut Vec<(String, String)>) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                collect_leaves(value, &format!("{at}.{key}"), into);
            }
        },
        toml::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_leaves(value, &format!("{at}[{index}]"), into);
            }
        },
        toml::Value::String(text) => into.push((at.to_owned(), text.clone())),
        toml::Value::Integer(number) => into.push((at.to_owned(), number.to_string())),
        toml::Value::Float(number) => into.push((at.to_owned(), number.to_string())),
        toml::Value::Boolean(yes) => into.push((at.to_owned(), yes.to_string())),
        toml::Value::Datetime(when) => into.push((at.to_owned(), when.to_string())),
    }
}
