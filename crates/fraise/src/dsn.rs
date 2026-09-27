//! Which database an invocation is about, by confiture's precedence contract.
//!
//! Three of the four tools want a DSN and each reads it differently, so before `fraise` runs any
//! of them it has to answer one question — *which database is this command about* — and answer it
//! the same way every time. The answer is not invented here. Confiture's **#152 precedence
//! contract** already decides it, in `python/confiture/cli/dsn.py::resolve_database_url`, and its
//! principle is written there in one line: **explicit-and-singular wins; ambiguity fails loud**.
//! That file is byte-identical in the pinned 1.19.0 and in the 1.24.0 checkout (measured
//! 2026-09-27), so it is a contract rather than a release's behaviour, and this module is it.
//!
//! `fraise`'s inputs are not confiture's, so the ladder below is the same rules through this
//! face's statements. Each rung says which of confiture's eight numbered steps it mirrors:
//!
//! | rung | stated as | mirrors | what it means |
//! |---|---|---|---|
//! | `named_variable` | `--database-url-env`, `FRAISE_DATABASE_URL_ENV` | step 1 | a source named on argv wins outright, as confiture's `--database-url` does |
//! | `chosen_environment` | `--environment`, `FRAISE_ENVIRONMENT` | step 4 | an environment of the document, chosen for this command |
//! | `canonical_variable` | `CONFITURE_DATABASE_URL` | step 5 | confiture's canonical variable, set on purpose, beats a document that merely defaults |
//! | `default_environment` | `project.default_environment` | step 6 | the document's own default, which beats an ambient DSN |
//! | `ambient_variable` | `DATABASE_URL` | step 7 | whatever the shell happens to hold — refused for a mutating command |
//! | `nothing` | — | step 8 | no source, which is an answer: the tool's own configuration may hold one |
//!
//! Two of confiture's steps are refusals rather than rungs, and both are adopted: two explicit
//! statements below the flag are never reconciled silently (step 3, `CONFIG_007`), and a mutating
//! command refuses an ambient-only DSN (step 7 under `require_intentional_source`, which is
//! confiture's own name for it). The contract lists both codes under one class, which is the
//! class this face's configuration refusal reads as — asserted against the vendored document in
//! `tests/dsn.rs` rather than taken on trust.
//!
//! Where the two deliberately differ, they differ in the direction that keeps a secret out of a
//! log. Confiture's step 1 is `--database-url`, a DSN on argv; this face will not take one, so
//! its top rung names the *variable* instead — argv sees a name, never a connection string. And
//! confiture has no `--no-config`, because a document `fraise` cannot read is a refusal here
//! rather than a fallback.
//!
//! **A rung that answers, answers.** If the variable a rung named is not set, that is a refusal
//! and not a fall-through to the next rung: a command that quietly ran against another database
//! is the failure this whole ladder exists to prevent. Confiture's step 6 has the same shape —
//! it defers to a config that is merely *present*, without asking whether that config's own DSN
//! turns out to be usable.
//!
//! **An empty variable is not a source.** Confiture's resolver tests its two variables for
//! truthiness (`if confiture_url:`), so `CONFITURE_DATABASE_URL=""` is no source at all. That is
//! not the same rule `${VAR}` expansion uses — there, a variable that is *present* expands, empty
//! or not (`if inner not in os.environ`) — and the difference is deliberate in confiture, so it
//! is deliberate here: [`crate::interpolation`] keeps the expansion rule and this module keeps the
//! ladder's.
//!
//! **A `FRAISE_` override names a source, never a value.** The two overrides above are the
//! environment spellings of the two flags that name a source, and both hold a *name*: which
//! environment, or which variable. There is no `FRAISE_` spelling for a setting's value, and that
//! is the rule rather than an omission — the document is the one place a value is written, it is
//! committed and diffed, and an override that could set one would be a second document nobody can
//! read. A value from the environment already has a declared way in, `${VAR}` in the document,
//! where a reader can see it. An override still owes [`Resolution::from_env`] its entry, because
//! it displaced part of the document a report shows as written.
//!
//! **An ambient DSN is never promoted to an intentional one.** What `fraise` hands a child is the
//! resolved DSN under every name the stack reads it by, but only when the source was intentional.
//! Copying an ambient `DATABASE_URL` into `CONFITURE_DATABASE_URL` would launder an accident into
//! an intention, and confiture's own refusal for a mutating command could never fire again.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

use crate::config::{Environment, Problem};
use crate::interpolation::{self, Vars};

/// Confiture's canonical DSN variable: the one a caller sets on purpose.
///
/// Named in its #152 contract (`cli/dsn.py`, `_CONFITURE_DSN_ENV`) and in the `--database-url`
/// help text every command in its migrate family shares. `tests/dsn.rs` holds the pinned
/// confiture to still documenting both of these names.
const CANONICAL: &str = "CONFITURE_DATABASE_URL";

/// The conventional DSN variable: ubiquitous, ambient, and what fraiseql reads.
///
/// Confiture treats it as the unintentional source (`_AMBIENT_DSN_ENV`); fraiseql's CLI resolves
/// its database from `--database`, then this, then `[database].url` in `fraiseql.toml`
/// (`crates/fraiseql-cli/src/commands/run.rs`, measured at 2.14.1).
const AMBIENT: &str = "DATABASE_URL";

/// The environment variable that names an environment for this command: `--environment`'s other
/// spelling.
const ENVIRONMENT_OVERRIDE: &str = "FRAISE_ENVIRONMENT";

/// The environment variable that names the DSN's variable for this command:
/// `--database-url-env`'s other spelling.
const VARIABLE_OVERRIDE: &str = "FRAISE_DATABASE_URL_ENV";

/// One rung of the ladder: a statement that can be made about where the DSN comes from, and what
/// it means when it is the highest statement made.
///
/// There are no branches to read: the rungs are in order, each names the one statement it reads,
/// and the first whose statement was made is the answer.
#[derive(Debug)]
pub struct Rung {
    name: &'static str,
    statement: Statement,
    stated_as: &'static str,
    conflicts: bool,
    intentional: bool,
    mirrors: &'static str,
    why: &'static str,
}

impl Rung {
    /// The token a report names this rung by.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// How this rung is stated, as a caller writes it.
    #[must_use]
    pub const fn stated_as(&self) -> &'static str {
        self.stated_as
    }

    /// Whether a DSN reached through this rung was named for this command on purpose.
    ///
    /// The one rung that is not intentional is the ambient variable, and that is the whole
    /// distinction #152 exists to draw.
    #[must_use]
    pub const fn is_intentional(&self) -> bool {
        self.intentional
    }

    /// Which step of confiture's contract this rung mirrors.
    #[must_use]
    pub const fn mirrors(&self) -> &'static str {
        self.mirrors
    }

    /// Why this rung sits where it sits.
    #[must_use]
    pub const fn why(&self) -> &'static str {
        self.why
    }
}

/// What can be stated about where the DSN comes from. One per rung, and no others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Statement {
    /// A variable named for this command.
    NamedVariable,
    /// An environment of the document, chosen for this command.
    ChosenEnvironment,
    /// Confiture's canonical variable is set.
    CanonicalVariable,
    /// The document defaults to an environment.
    DefaultEnvironment,
    /// The conventional variable is set, by whatever set it.
    AmbientVariable,
    /// Nothing, which every invocation states.
    Nothing,
}

/// The ladder, in order. Confiture's contract with this face's statements in it.
const LADDER: [Rung; 6] = [
    Rung {
        name: "named_variable",
        statement: Statement::NamedVariable,
        stated_as: "--database-url-env (or FRAISE_DATABASE_URL_ENV)",
        conflicts: false,
        intentional: true,
        mirrors: "confiture's step 1",
        why: "a source named for this command wins outright, as confiture's --database-url does — \
               and it is a variable's name rather than a DSN, because argv is read by everything \
               on the machine",
    },
    Rung {
        name: "chosen_environment",
        statement: Statement::ChosenEnvironment,
        stated_as: "--environment (or FRAISE_ENVIRONMENT)",
        conflicts: true,
        intentional: true,
        mirrors: "confiture's step 4",
        why: "an environment of the document, chosen for this command, which an ambient DSN never \
               overrides",
    },
    Rung {
        name: "canonical_variable",
        statement: Statement::CanonicalVariable,
        stated_as: CANONICAL,
        conflicts: true,
        intentional: true,
        mirrors: "confiture's step 5",
        why: "confiture's canonical variable is set on purpose, so it beats a document that merely \
               defaults",
    },
    Rung {
        name: "default_environment",
        statement: Statement::DefaultEnvironment,
        stated_as: "a project.default_environment in fraise.toml",
        conflicts: false,
        intentional: true,
        mirrors: "confiture's step 6",
        why: "the document's own default, which beats an ambient DSN as a present config does",
    },
    Rung {
        name: "ambient_variable",
        statement: Statement::AmbientVariable,
        stated_as: AMBIENT,
        conflicts: false,
        intentional: false,
        mirrors: "confiture's step 7",
        why: "whatever the environment happens to hold, which nothing named for this command — a \
               mutating command refuses it rather than running against it",
    },
    Rung {
        name: "nothing",
        statement: Statement::Nothing,
        stated_as: "nothing",
        conflicts: false,
        intentional: false,
        mirrors: "confiture's step 8",
        why: "no source, which is an answer rather than a refusal: the tool's own configuration \
               may still hold one",
    },
];

/// The rung every invocation reaches: it states nothing, and nothing is an answer.
const NOTHING: &Rung = &LADDER[LADDER.len() - 1];

/// What the command line said about the database.
///
/// The environment's own spellings of these are read by [`resolve`] rather than by the parser, so
/// that the ladder can say which of the two decided — a report that could not tell an override
/// from a flag would be a report that hides where the answer came from.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flags<'a> {
    /// `--environment NAME`.
    pub environment: Option<&'a str>,
    /// `--database-url-env NAME`.
    pub variable: Option<&'a str>,
    /// Whether the invocation changes the database.
    ///
    /// A property of the invocation `fraise` builds, never a list of verb names: `fraise tool`
    /// hands a tool arguments `fraise` did not write, so its caller is the only one who knows,
    /// exactly as `--payload` is the caller's to say. When the verbs arrive, each one states its
    /// own, because `fraise` wrote their arguments.
    pub mutating: bool,
}

/// What the document declares about environments.
#[derive(Debug, Clone, Copy)]
pub enum Declared<'a> {
    /// There is no `fraise.toml`, so nothing declares an environment.
    NoDocument,
    /// What the document in hand declares.
    Document {
        /// `project.default_environment`, already held at load to naming one of the below.
        default: Option<&'a str>,
        /// `[environments.<name>]`, whose `database_url_env` is a variable's name and never a
        /// DSN — held to that when the document was read, which is why handing one out here
        /// cannot hand out a value.
        environments: &'a BTreeMap<String, Environment>,
    },
}

impl<'a> Declared<'a> {
    /// The environment of that name, or `None` when nothing declares one.
    fn environment(self, name: &str) -> Option<&'a Environment> {
        match self {
            Self::NoDocument => None,
            Self::Document { environments, .. } => environments.get(name),
        }
    }

    /// The environment a command falls back to when it names none.
    const fn default(self) -> Option<&'a str> {
        match self {
            Self::NoDocument => None,
            Self::Document { default, .. } => default,
        }
    }

    /// What there is to choose from, for a refusal to name.
    fn choices(self) -> String {
        match self {
            Self::NoDocument => "there is no fraise.toml here, so it declares none".to_owned(),
            Self::Document { environments, .. } if environments.is_empty() => {
                "the file declares none".to_owned()
            },
            Self::Document { environments, .. } => {
                environments.keys().cloned().collect::<Vec<String>>().join(", ")
            },
        }
    }
}

/// Which database this invocation is about: the rung that answered, and what it named.
///
/// It holds no DSN. The value is read once, at the moment a tool is about to run, by
/// [`Resolution::handover`] — so a resolution can be reported, logged and serialised without
/// anything having looked at a secret.
#[derive(Debug)]
pub struct Resolution {
    rung: &'static Rung,
    environment: Option<String>,
    variable: Option<String>,
    declared_variable: Option<String>,
    set: bool,
    mutating: bool,
    from_env: BTreeMap<String, Vec<String>>,
}

impl Resolution {
    /// The rung that answered.
    #[must_use]
    pub const fn rung(&self) -> &'static Rung {
        self.rung
    }

    /// The environment in force, or `None` when nothing named one.
    #[must_use]
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    /// The variable that carries this invocation's DSN, or `None` when there is no source.
    #[must_use]
    pub fn variable(&self) -> Option<&str> {
        self.variable.as_deref()
    }

    /// Whether that variable is set where `fraise` is running.
    ///
    /// A fact about the machine at this moment rather than about the document, which is why
    /// reporting it is not a refusal: `config show` says so and exits zero, and the refusal
    /// belongs where the DSN is needed.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.set
    }

    /// Which variables of the environment decided a source, by what they decided.
    ///
    /// An override is a statement from the environment, so it owes the same entry a `${VAR}`
    /// does: a report that showed the document as written without saying an override displaced
    /// part of it would be a report about a configuration nobody is running.
    #[must_use]
    pub const fn from_env(&self) -> &BTreeMap<String, Vec<String>> {
        &self.from_env
    }

    /// The resolution as a report: for a person, for the envelope, and for an agent deciding
    /// what to set. Every field is a name or a fact; none of them is the DSN.
    #[must_use]
    pub fn report(&self) -> Report<'_> {
        Report {
            rung: self.rung.name(),
            why: self.rung.why(),
            mirrors: self.rung.mirrors(),
            intentional: self.rung.is_intentional(),
            environment: self.environment(),
            variable: self.variable(),
            set: self.set,
        }
    }

    /// Read the DSN and say what to add to a child's environment.
    ///
    /// This is the only place a DSN is read, and it is read at the moment a tool is about to run.
    ///
    /// # Errors
    ///
    /// If the variable the ladder named is not set — a rung that answers, answers — or if the
    /// only source is an ambient one and the invocation changes the database.
    pub fn handover(&self, vars: &dyn Vars) -> Result<Handover, Problem> {
        if !self.rung.intentional {
            return if self.mutating {
                Err(Problem::new(format!(
                    "{} is the only DSN here, and nothing named it for this command. A command \
                     that changes the database refuses an ambient one rather than running against \
                     whatever is exported: name the source with {}",
                    self.rung.stated_as,
                    intentional_spellings()
                )))
            } else {
                Ok(Handover::nothing())
            };
        }
        let Some(variable) = self.variable() else {
            return Ok(Handover::nothing());
        };
        let Some(dsn) = value(vars, variable) else {
            let named = self.environment().map_or_else(
                || format!("{} names {variable}", self.rung.stated_as),
                |environment| format!("environment {environment:?} names {variable}"),
            );
            return Err(Problem::new(format!(
                "{named}, and it is not set where fraise is running. The DSN is not looked for \
                 anywhere else — a command that quietly ran against another database is what this \
                 refuses — so export it, or name another source with {}",
                intentional_spellings()
            )));
        };

        // One DSN, under every name the stack reads it by: confiture's canonical variable, the
        // conventional one fraiseql reads, and the name the project's own document gave it, which
        // is the name fraisier resolves through its config. Two tools reading two databases
        // inside one command is what this prevents.
        let mut names: Vec<String> = Vec::new();
        for name in [CANONICAL.to_owned(), AMBIENT.to_owned()]
            .into_iter()
            .chain(self.declared_variable.clone())
        {
            // The one already resolved is where the DSN came from, so setting it again would say
            // nothing; a name given twice would too.
            if name != variable && !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(Handover {
            variables: names.into_iter().map(|name| (name, dsn.clone())).collect(),
        })
    }
}

/// What `config show` says about the database: the ladder's answer, and nothing that is a secret.
#[derive(Debug, Serialize)]
pub struct Report<'a> {
    /// The rung that answered.
    rung: &'a str,
    /// Why it is the one that answers.
    why: &'a str,
    /// Which step of confiture's contract it mirrors.
    mirrors: &'a str,
    /// Whether the source was named for this command.
    intentional: bool,
    /// The environment in force, or null.
    environment: Option<&'a str>,
    /// The variable that carries the DSN, or null.
    variable: Option<&'a str>,
    /// Whether that variable is set where `fraise` is running.
    set: bool,
}

/// The DSN, under the names the stack reads it by: what `fraise` adds to a child's environment.
///
/// A DSN reaches a tool this way and no other — never on argv, where every process on the
/// machine can read it. Its [`fmt::Debug`] prints the names alone for the same reason: a type
/// that held a secret and printed it in a log would only need one `{:?}` to undo the rule.
pub struct Handover {
    variables: Vec<(String, String)>,
}

impl Handover {
    /// Nothing to add: either the ladder found no source, or what it found was already in the
    /// environment the child inherits.
    #[must_use]
    pub const fn nothing() -> Self {
        Self {
            variables: Vec::new(),
        }
    }

    /// The variables to set on the child, in the order they were decided.
    pub fn variables(&self) -> impl Iterator<Item = (&str, &str)> {
        self.variables.iter().map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// The names this handover sets, which is the part of it that is safe to print.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.variables.iter().map(|(name, _)| name.as_str()).collect()
    }
}

impl fmt::Debug for Handover {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handover").field("sets", &self.names()).finish()
    }
}

/// Which database this invocation is about.
///
/// # Errors
///
/// If two explicit statements disagree, if a chosen environment is not one the document
/// declares, or if a variable is named in a form confiture would not expand.
pub fn resolve(
    flags: Flags<'_>,
    declared: Declared<'_>,
    vars: &dyn Vars,
) -> Result<Resolution, Problem> {
    let mut from_env = BTreeMap::new();
    let named = match stated(
        "--database-url-env",
        flags.variable,
        VARIABLE_OVERRIDE,
        "database.variable",
        vars,
        &mut from_env,
    ) {
        Some((at, name)) => Some(variable_name(at, &name)?),
        None => None,
    };
    let chosen = stated(
        "--environment",
        flags.environment,
        ENVIRONMENT_OVERRIDE,
        "database.environment",
        vars,
        &mut from_env,
    )
    .map(|(_, name)| name);
    if let Some(chosen) = &chosen {
        if declared.environment(chosen).is_none() {
            return Err(Problem::new(format!(
                "{chosen:?} is not an environment this project declares: {}",
                declared.choices()
            )));
        }
    }

    // Which statements this invocation made, as a set, so that a rung is a lookup rather than a
    // branch — and the last rung is one every invocation there is makes.
    let mut made = BTreeSet::from([Statement::Nothing]);
    for (statement, was_made) in [
        (Statement::NamedVariable, named.is_some()),
        (Statement::ChosenEnvironment, chosen.is_some()),
        (Statement::CanonicalVariable, value(vars, CANONICAL).is_some()),
        (Statement::DefaultEnvironment, declared.default().is_some()),
        (Statement::AmbientVariable, value(vars, AMBIENT).is_some()),
    ] {
        if was_made {
            made.insert(statement);
        }
    }
    let rung = LADDER.iter().find(|rung| made.contains(&rung.statement)).unwrap_or(NOTHING);

    // Confiture checks its two-explicit-sources conflict *after* the flag has already won, so a
    // command that named a variable is never refused for the statements made beside it.
    if rung.statement != Statement::NamedVariable {
        let conflicting: Vec<&Rung> = LADDER
            .iter()
            .filter(|rung| rung.conflicts && made.contains(&rung.statement))
            .collect();
        if conflicting.len() > 1 {
            return Err(Problem::new(format!(
                "{} both say where this command's DSN comes from, and two explicit sources are \
                 never reconciled silently — which of them did you mean? Pass exactly one, or \
                 name a variable with --database-url-env",
                conflicting
                    .iter()
                    .map(|rung| rung.stated_as)
                    .collect::<Vec<&'static str>>()
                    .join(" and ")
            )));
        }
    }

    let environment = chosen.or_else(|| declared.default().map(ToOwned::to_owned));
    // The name the environment in force gave its DSN. It is what the environment rungs resolve
    // to, and — whichever rung answered — the name fraisier will look the DSN up by, since the
    // config this face renders for it carries that name.
    let declared_variable = environment
        .as_deref()
        .and_then(|name| declared.environment(name))
        .map(|environment| environment.database_url_env().to_owned());
    let variable = match rung.statement {
        Statement::NamedVariable => named,
        Statement::ChosenEnvironment | Statement::DefaultEnvironment => declared_variable.clone(),
        Statement::CanonicalVariable => Some(CANONICAL.to_owned()),
        Statement::AmbientVariable => Some(AMBIENT.to_owned()),
        Statement::Nothing => None,
    };
    let set = variable.as_deref().is_some_and(|name| value(vars, name).is_some());

    Ok(Resolution {
        rung,
        environment,
        variable,
        declared_variable,
        set,
        mutating: flags.mutating,
        from_env,
    })
}

/// One statement, which a caller can make twice over: on argv, or as the `FRAISE_` override that
/// is the same statement in the environment.
///
/// argv wins, because it is the more explicit spelling of the same thing. The override records
/// itself in `from_env` under `path`, because a report that showed the document as written
/// without saying an override displaced part of it would be a report about a configuration
/// nobody is running. What comes back is where the statement was made, which is what a refusal
/// about it has to name.
fn stated(
    flag: &'static str,
    on_argv: Option<&str>,
    override_name: &'static str,
    path: &str,
    vars: &dyn Vars,
    from_env: &mut BTreeMap<String, Vec<String>>,
) -> Option<(&'static str, String)> {
    if let Some(on_argv) = on_argv {
        return Some((flag, on_argv.to_owned()));
    }
    let found = value(vars, override_name)?;
    from_env.insert(path.to_owned(), vec![override_name.to_owned()]);
    Some((override_name, found))
}

/// What a variable holds, treating empty as unset — confiture's own rule for this ladder.
fn value(vars: &dyn Vars, name: &str) -> Option<String> {
    vars.get(name).filter(|value| !value.is_empty())
}

/// How every refusal here says what would have named a source: the ladder's own intentional
/// rungs, so the advice cannot drift from the order that produced it.
fn intentional_spellings() -> String {
    let spellings: Vec<&'static str> =
        LADDER.iter().filter(|rung| rung.intentional).map(Rung::stated_as).collect();
    match spellings.split_last() {
        Some((last, before)) if !before.is_empty() => format!("{}, or {last}", before.join(", ")),
        _ => spellings.join(", "),
    }
}

/// Hold a statement that must be an environment variable's name to being one.
///
/// The same strict form the document is held to, for the same reason: what `fraise` looks up and
/// what confiture would expand cannot be two different ideas of a name. A rejected value is
/// quoted back only when it cannot be a connection string.
fn variable_name(at: &str, value: &str) -> Result<String, Problem> {
    if interpolation::is_variable_name(value) {
        return Ok(value.to_owned());
    }
    if interpolation::is_plain_identifier(value) {
        return Err(Problem::new(format!(
            "{at} takes the name of an environment variable, and {value:?} is not one: a name is \
             [A-Z_][A-Z0-9_]*, the form confiture expands and fraise looks up. Did you mean {:?}?",
            value.to_ascii_uppercase()
        )));
    }
    Err(Problem::new(format!(
        "{at} takes the name of an environment variable — [A-Z_][A-Z0-9_]*, the form confiture \
         expands and fraise looks up — and what was passed is not one. It is not quoted back \
         here, because a connection string in a refusal is a connection string in a log: pass the \
         name of the variable that carries the DSN, and set that variable where the command runs"
    )))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        AMBIENT, CANONICAL, Declared, Flags, LADDER, NOTHING, Resolution, Rung, Statement, resolve,
    };
    use crate::pinned;

    /// A resolution against a stated environment and a document that declares nothing, which is
    /// the shape the invariants below are about.
    fn nothing_declared(vars: &[(&str, &str)]) -> Resolution {
        let vars: BTreeMap<String, String> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        resolve(Flags::default(), Declared::NoDocument, &vars).expect("nothing is stated")
    }

    #[test]
    fn every_rung_reads_a_statement_of_its_own() {
        // The ladder is only data if each row answers for exactly one statement: two rows on the
        // same one would make the order between them unreachable, and a report would name a rung
        // that cannot be the one that decided.
        let statements: BTreeSet<Statement> = LADDER.iter().map(|rung| rung.statement).collect();
        assert_eq!(statements.len(), LADDER.len(), "{LADDER:#?}");

        let names: BTreeSet<&str> = LADDER.iter().map(Rung::name).collect();
        assert_eq!(names.len(), LADDER.len(), "a rung's name is what a report says: {names:?}");
    }

    #[test]
    fn the_last_rung_is_the_one_every_invocation_reaches() {
        // `NOTHING` is the bottom of the ladder by index, so this is what keeps the index honest
        // if a rung is ever appended.
        assert_eq!(NOTHING.statement, Statement::Nothing);
        assert!(!NOTHING.intentional, "nothing is not a source anyone named");
    }

    #[test]
    fn the_rungs_are_in_the_order_of_the_steps_they_mirror() {
        // The ladder's claim is that it is confiture's contract, and a contract is an order. Each
        // rung names the step it mirrors, so the order of those numbers is checkable.
        let steps: Vec<u32> = LADDER
            .iter()
            .map(|rung| {
                rung.mirrors
                    .rsplit(' ')
                    .next()
                    .and_then(|step| step.parse().ok())
                    .unwrap_or_else(|| panic!("{} names no step of the contract", rung.name))
            })
            .collect();
        let mut ascending = steps.clone();
        ascending.sort_unstable();
        assert_eq!(steps, ascending, "the rungs run against confiture's own order: {steps:?}");
    }

    #[test]
    fn an_empty_variable_is_not_a_source() {
        // Confiture's resolver tests its two variables for truthiness, so an empty one is no
        // source at all — which is not the rule `${VAR}` expansion uses, where a variable that is
        // present expands whatever it holds. Both rules are confiture's, and each is kept where
        // it belongs.
        assert_eq!(nothing_declared(&[(CANONICAL, "")]).rung().name(), "nothing");
        assert_eq!(nothing_declared(&[(AMBIENT, "")]).rung().name(), "nothing");
        assert_eq!(nothing_declared(&[(AMBIENT, "postgresql:///x")]).rung().name(), AMBIENT_RUNG);
    }

    /// The rung an ambient variable answers on, named once so the two tests below agree.
    const AMBIENT_RUNG: &str = "ambient_variable";

    #[test]
    fn nothing_is_handed_over_for_a_source_nobody_named() {
        // The asymmetry #152 exists to draw: an ambient DSN is left exactly as it is. Promoting it
        // would launder an accident into an intention and confiture's own refusal could never
        // fire again.
        let vars = BTreeMap::from([(AMBIENT.to_owned(), "postgresql:///x".to_owned())]);
        let resolution = nothing_declared(&[(AMBIENT, "postgresql:///x")]);
        let handover = resolution.handover(&vars).expect("a reading may proceed");
        assert!(handover.names().is_empty(), "{handover:?}");
    }

    #[test]
    fn a_handover_prints_the_names_it_sets_and_never_what_it_sets_them_to() {
        // One `{:?}` in a log is all it would take, so the type cannot print a DSN at all.
        let vars = BTreeMap::from([("APP_DSN".to_owned(), "postgresql://u:s3cret@h/d".to_owned())]);
        let resolution = resolve(
            Flags {
                variable: Some("APP_DSN"),
                ..Flags::default()
            },
            Declared::NoDocument,
            &vars,
        )
        .expect("a named variable is a source");
        let handover = resolution.handover(&vars).expect("and it is set");

        let shown = format!("{handover:?}");
        assert!(shown.contains(CANONICAL) && shown.contains(AMBIENT), "{shown}");
        assert!(!shown.contains("s3cret"), "a DSN must not be printable: {shown}");
        assert_eq!(handover.names(), vec![CANONICAL, AMBIENT], "under both names the stack reads");
    }

    #[test]
    fn the_pinned_confiture_still_names_the_two_variables_this_ladder_reads() {
        // The names in this module are a claim about another tool's interface, so they get what
        // every claim of that kind gets here: a measurement against the pinned release that fails
        // rather than skips. Confiture documents both in the option help its #152 contract shares
        // across the migrate family — `--database-url` for the canonical one beating a default
        // config, `--no-config` for the order between the two.
        let help = pinned::output(&["migrate", "up", "--help"]);
        let joined: String = help.chars().filter(|character| !character.is_whitespace()).collect();
        for name in [CANONICAL, AMBIENT] {
            assert!(
                joined.contains(name),
                "confiture {} no longer documents {name} in `migrate up --help`, so what this \
                 module hands a child may no longer be what it reads.\n{}",
                pinned::version(),
                pinned::install_hint(&pinned::version())
            );
        }
    }
}
