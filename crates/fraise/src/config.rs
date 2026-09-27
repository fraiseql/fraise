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
//! carries its DSN — `database_url_env`, whose value must be an environment variable's name, in
//! the same strict form [`crate::interpolation`] expands — so a password cannot be committed by
//! writing it here, and `fraise` reads the variable at the moment it runs a tool. The refusal
//! that enforces it prints the key and never the value: a refusal that quoted the DSN would put
//! it in the log the rule exists to keep it out of.
//!
//! What is expanded and what is read as written is the one thing to know beyond that. The
//! passthrough tables are the tools' own settings, and a host, a port or a token in one of them
//! legitimately comes from the environment, so every string in them is expanded. The fields
//! `fraise` reads itself — a project's name, the environment to default to, the name of a
//! variable — are read as written and refuse a `${…}`, because they are resolved against this
//! document rather than against the machine, and a file whose own names need an environment to
//! be read is a file nobody can read. Between them, no string in a loaded document keeps an
//! unexpanded reference.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::dsn::{Declared, Report, Resolution};
use crate::envelope::Payload;
use crate::exit_table::{ExitTable, Refusal};
use crate::interpolation::{self, Vars};

/// The file's name.
///
/// `fraise` looks for it in one directory: the one it was told to work in. Searching upwards
/// would mean a verb whose configuration depends on where it was invoked from, and the
/// umbrella's directory is a decision its caller makes explicitly.
pub const FILE: &str = "fraise.toml";

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
    /// A refusal about the configuration in hand.
    ///
    /// Reachable from [`crate::dsn`] as well as from here: which database a command is about is
    /// part of its configuration, and confiture — whose ladder that module is — classes its own
    /// refusals about it the same way.
    pub(crate) const fn new(message: String) -> Self {
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
        Self::find(directory)?.ok_or_else(|| {
            Problem::new(format!(
                "there is no {FILE} in {}, so there is no project here to read",
                directory.display()
            ))
        })
    }

    /// Read the `fraise.toml` of the project in `directory`, or `None` when there is no project
    /// there.
    ///
    /// The distinction a dispatch needs: `fraise tool` reaches a tool whether or not the
    /// directory is a project — that is what makes it the fallback this face promises — but a
    /// document that *is* there is read, and read whole, because a command about to touch a
    /// database must not be the one that ignored the file saying which database.
    ///
    /// # Errors
    ///
    /// If there is a document and it is not one this face can act on.
    pub fn find(directory: &Path) -> Result<Option<Self>, Problem> {
        let path = directory.join(FILE);
        match fs::read_to_string(&path) {
            Ok(source) => Self::read(&source, path).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(Problem::new(format!("{FILE} cannot be read: {}: {error}", path.display())))
            },
        }
    }

    /// What this document declares about environments, for the ladder that decides which
    /// database a command is about.
    ///
    /// Handing these out is not handing out a value: the fields in them are the ones this face
    /// reads itself, held at load to being written rather than referenced, and
    /// `database_url_env` to being a variable's *name*.
    #[must_use]
    pub fn declared(&self) -> Declared<'_> {
        Declared::Document {
            default: self.project.default_environment(),
            environments: &self.environments,
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

    /// What this document declares about environments.
    #[must_use]
    pub fn declared(&self) -> Declared<'_> {
        Declared::Document {
            default: self.project.default_environment(),
            environments: &self.environments,
        }
    }

    /// The document as a reader may see it: as written, plus which variables fed which setting,
    /// plus which database the invocation being reported is about.
    ///
    /// `database`'s own `from_env` entries are merged into this document's, because an override
    /// that displaced part of the document is a value from the environment like any other — and
    /// a report that showed the document as written without saying so would be a report about a
    /// configuration nobody is running.
    #[must_use]
    pub fn view<'a>(&'a self, database: &'a Resolution) -> View<'a> {
        let mut from_env = self.from_env.clone();
        for (path, variables) in database.from_env() {
            from_env.insert(path.clone(), variables.clone());
        }
        View {
            path: &self.path,
            project: &self.project,
            environments: &self.environments,
            tools: &self.written,
            database: database.report(),
            from_env,
        }
    }

    /// The view as the envelope's payload: a document, because `fraise` produced it itself.
    ///
    /// # Panics
    ///
    /// If the view cannot be serialised, which is a bug in this module rather than a state a
    /// machine can be in.
    #[must_use]
    pub fn payload(&self, database: &Resolution) -> Payload {
        Payload::Json(serde_json::to_value(self.view(database)).expect("a view serialises"))
    }

    /// The view as a person reads it.
    #[must_use]
    pub fn render(&self, database: &Resolution) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "{} — {}", self.project.name, self.path.display());
        if let Some(default) = &self.project.default_environment {
            let _ = writeln!(text, "default environment: {default}");
        }

        if !self.environments.is_empty() {
            text.push_str("\nenvironments\n");
            let width = widest(self.environments.keys().map(String::as_str));
            for (name, environment) in &self.environments {
                let _ = writeln!(text, "  {name:width$}  {}", environment.database_url_env);
            }
        }

        for (tool, table) in &self.written {
            let _ = writeln!(text, "\n[{tool}]");
            let leaves = flattened(table, tool);
            let width = widest(leaves.iter().map(|(path, _)| path.as_str()));
            for (path, value) in &leaves {
                let _ = writeln!(text, "  {path:width$}  {value}");
            }
        }

        let rung = database.rung();
        text.push_str("\ndatabase\n");
        let _ = writeln!(text, "  rung         {}, which mirrors {}", rung.name(), rung.mirrors());
        if let Some(environment) = database.environment() {
            let _ = writeln!(text, "  environment  {environment}");
        }
        if let Some(variable) = database.variable() {
            let set = if database.is_set() { "set" } else { "not set" };
            let _ = writeln!(text, "  variable     {variable} ({set})");
        }

        let mut from_env = self.from_env.clone();
        for (path, variables) in database.from_env() {
            from_env.insert(path.clone(), variables.clone());
        }
        if !from_env.is_empty() {
            text.push_str("\nfrom the environment, and not shown here\n");
            let width = widest(from_env.keys().map(String::as_str));
            for (path, variables) in &from_env {
                let _ = writeln!(text, "  {path:width$}  {}", variables.join(", "));
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
    database: Report<'a>,
    from_env: BTreeMap<String, Vec<String>>,
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
    if interpolation::is_variable_name(value) {
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
    let quoted = if interpolation::is_plain_identifier(value) {
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
            let (resolved, used) = interpolation::expanded(text, at, vars).map_err(Problem::new)?;
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

/// The longest of `words`, which is what the column before a value is padded to. A report whose
/// second column starts in a different place on every line is one a reader scans instead of
/// reads, and `doctor`'s findings are aligned for the same reason.
fn widest<'a>(words: impl Iterator<Item = &'a str>) -> usize {
    words.map(str::len).max().unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{Config, Loaded};
    use crate::compatibility::{CompatibilityTable, Tool};
    use crate::dsn::{self, Flags, Resolution};

    /// The resolution a document's own report is made against: no flags, and an environment
    /// holding nothing, so what these tests assert is the document and never the machine.
    fn about(loaded: &Loaded) -> Resolution {
        dsn::resolve(Flags::default(), loaded.declared(), &BTreeMap::new())
            .expect("a document with no environments states nothing about a database")
    }

    /// A document, read and resolved against a stated environment.
    fn load(source: &str, vars: &[(&str, &str)]) -> Result<Loaded, String> {
        let vars: BTreeMap<String, String> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        Config::read(source, PathBuf::from("fraise.toml"))
            .and_then(|config| config.resolve(&vars))
            .map_err(|problem| problem.message().to_owned())
    }

    #[test]
    fn a_passthrough_table_exists_for_every_tool_the_umbrella_speaks_for() {
        // The two documents of this crate have to agree on what the stack is: a tool `fraise`
        // dispatches to and has no table for would be a tool nobody can configure through the
        // one config file, and a table for a tool the umbrella does not speak for would be
        // settings nothing ever reads.
        let loaded =
            load("[project]\nname = \"p\"\n[confiture]\n[fraiseql]\n[fraisier]\n[specql]\n", &[])
                .expect("the document is whole");
        let named: Vec<&str> = CompatibilityTable::vendored().tools().map(Tool::name).collect();
        for tool in &named {
            assert!(loaded.settings(tool).is_some(), "{tool} has no passthrough table");
        }
        assert_eq!(loaded.written.len(), named.len(), "and there are no others: {named:?}");
    }

    #[test]
    fn a_reference_is_found_wherever_a_tools_own_table_puts_one() {
        // A tool's table is the tool's shape, not ours, so the walk goes through nested tables
        // and arrays — and the path it records is the one a refusal names and a reader greps.
        let loaded = load(
            "[project]\nname = \"p\"\n\n[confiture]\nurl = \"${HOOK}\"\nhosts = [\"${PGHOST}\", \
             \"replica\"]\n\n[confiture.notifications]\nto = \"${MAILBOX}\"\n",
            &[
                ("HOOK", "https://hooks"),
                ("PGHOST", "db.internal"),
                ("MAILBOX", "ops@example"),
            ],
        )
        .expect("every reference resolves");

        let paths: Vec<&str> = loaded.from_env.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "confiture.hosts[0]",
                "confiture.notifications.to",
                "confiture.url"
            ],
            "{:?}",
            loaded.from_env
        );
        assert_eq!(
            loaded.settings("confiture").and_then(|table| table["url"].as_str()),
            Some("https://hooks"),
            "the settings a tool is handed are the resolved ones"
        );
        assert_eq!(
            loaded.view(&about(&loaded)).tools["confiture"]["url"].as_str(),
            Some("${HOOK}"),
            "and what a reader is shown is the reference"
        );
    }

    #[test]
    fn a_field_fraise_reads_itself_refuses_a_reference_rather_than_keeping_it() {
        // The other half of "no string in a loaded document keeps an unexpanded reference": the
        // passthrough tables expand, and the face's own fields refuse. Neither leaves one.
        let refusal = load("[project]\nname = \"${PROJECT}\"\n", &[("PROJECT", "p")])
            .expect_err("project.name is not expanded");
        assert!(refusal.contains("project.name"), "{refusal}");
    }

    #[test]
    fn an_environment_whose_name_could_not_be_a_file_name_is_refused() {
        // Phase 03 renders `db/environments/<name>.yaml`, so a name is held to what a file name
        // can be before anything is written — including the traversal a `..` would be.
        let refusal = load(
            "[project]\nname = \"p\"\n\n[environments.\"../../etc\"]\ndatabase_url_env = \"DB\"\n",
            &[],
        )
        .expect_err("that cannot name a file");
        assert!(refusal.contains("../../etc") && refusal.contains("file name"), "{refusal}");
    }

    #[test]
    fn an_environment_is_the_name_of_a_variable_and_a_lowercase_one_is_named_back() {
        let refusal = load(
            "[project]\nname = \"p\"\n\n[environments.local]\ndatabase_url_env = \"database_url\"\n",
            &[],
        )
        .expect_err("a name is the strict form");
        assert!(
            refusal.contains("environments.local.database_url_env")
                && refusal.contains("DATABASE_URL"),
            "the refusal names the key and the name it probably meant: {refusal}"
        );
    }
}
