//! `${VAR}` in a value, by confiture's rule rather than by one of our own.
//!
//! `fraise` renders the YAML confiture reads, so a reference this face accepted and confiture
//! rejected would be a document that loads here and fails one layer down — and a reference both
//! accepted but expanded differently would be worse, since nothing would fail at all. The rule
//! is therefore copied from `confiture.config._env_vars` and measured against the pinned
//! release (byte-identical in 1.19.0 and 1.23.1):
//!
//! | written | confiture | `fraise` |
//! |---|---|---|
//! | `${A}`, `A` set | expands | expands |
//! | `${A}`, `A` unset | refuses; a missing variable never expands to nothing | the same |
//! | `${lower}`, `${1A}` | refuses, naming the strict form | the same |
//! | `${A:-default}` | refuses; a shell default is not supported | the same |
//! | `${}` | refuses | the same |
//! | `$A` | left alone: only `${…}` is a reference | the same |
//! | a value that expands into `${…}` | refuses; expansion is single-pass | the same |
//! | `${B` alone | refuses; unclosed | the same |
//! | `${A} ${B` | **expands, leaving `${B`** | **refuses** |
//!
//! The last row is the one difference and it is deliberate: confiture looks for an unclosed
//! `${` only at the first one in the value, so a second survives into the rendered file. The
//! direction that has to be safe is this one — every document `fraise` accepts must be one
//! confiture accepts, not the reverse — so nothing here leaves a `${` in a resolved value.
//!
//! Refusals are sentences rather than a type: each one names the setting it is about, which is
//! the path its caller was walking, and the caller is what gives that sentence its exit.

/// Where a `${VAR}` is looked up.
///
/// A trait rather than a call straight into [`std::env`], so that a test states the environment
/// it means instead of mutating the one its process shares with every other test in the binary.
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

impl Vars for std::collections::BTreeMap<String, String> {
    fn get(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// Whether `text` is the name of an environment variable in the strict form confiture's
/// `${VAR}` accepts: `[A-Z_][A-Z0-9_]*`.
///
/// Public because it is not only the expander's rule: a field that holds the *name* of a
/// variable is held to the same form, so that what `fraise` looks up and what confiture would
/// expand cannot be two different ideas of a name.
#[must_use]
pub fn is_variable_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase() || first == '_')
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Whether `text` is a plain identifier.
///
/// The only shape of a rejected value that is safe to quote back in a refusal: anything holding
/// a `:`, a `/`, an `@` or a space may be a connection string, and a refusal that quoted one
/// would put a password in a log.
#[must_use]
pub fn is_plain_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// `value` with every reference expanded, and the variables that were read.
///
/// The scan is confiture's: anything shaped like a reference is found first and only then held
/// to the strict form, so a near miss is refused rather than left in the value for a tool to
/// receive verbatim. `at` is the setting being expanded, which every refusal names.
///
/// # Errors
///
/// With the sentence to show, if a reference is not the strict form, names a variable that is
/// not set, or resolves to a value that itself holds a reference. What a variable holds is
/// never quoted: the refusal names the variable instead.
pub fn expanded(value: &str, at: &str, vars: &dyn Vars) -> Result<(String, Vec<String>), String> {
    let mut resolved = String::with_capacity(value.len());
    let mut used = Vec::new();
    let mut rest = value;

    while let Some(open) = rest.find("${") {
        resolved.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find('}') else {
            return Err(format!(
                "{at} holds a `${{` with no closing `}}`, so what it refers to cannot be read"
            ));
        };
        let name = &after[..close];
        if !is_variable_name(name) {
            return Err(diagnosis(name, at));
        }
        let Some(found) = vars.get(name) else {
            return Err(format!(
                "{at} refers to ${{{name}}}, which is not set. A missing variable is refused \
                 rather than expanded to nothing, so a tool is never handed an empty setting"
            ));
        };
        if found.contains("${") {
            return Err(format!(
                "{at} refers to ${{{name}}}, whose value holds a reference of its own. Expansion \
                 is single-pass here as it is in confiture: resolve the nesting in the \
                 environment rather than in the file"
            ));
        }
        used.push(name.to_owned());
        resolved.push_str(&found);
        rest = &after[close + 1..];
    }
    resolved.push_str(rest);
    Ok((resolved, used))
}

/// Why a reference that is not the strict form is refused, in the terms confiture refuses it —
/// including the hint, since a lowercase name is the mistake people actually make.
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
             Write ${{{}}} and set it, or write the value in the file",
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{expanded, is_variable_name};

    /// The environment every case below resolves against, stated rather than inherited.
    fn vars() -> BTreeMap<String, String> {
        [
            ("A".to_owned(), "alpha".to_owned()),
            ("A_1".to_owned(), "one".to_owned()),
            ("NEST".to_owned(), "${A}".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    /// The module's table, as assertions. Each row was run against the pinned confiture's
    /// `expand_env_vars` before it was written here, so this is a record of a measurement and
    /// not of an expectation — including the one row where the two disagree on purpose.
    #[test]
    fn the_rule_is_the_one_the_pinned_confiture_applies() {
        let vars = vars();
        let expand = |value: &str| expanded(value, "a.setting", &vars);

        assert_eq!(expand("${A}"), Ok(("alpha".to_owned(), vec!["A".to_owned()])));
        assert_eq!(expand("${A_1}"), Ok(("one".to_owned(), vec!["A_1".to_owned()])));
        assert_eq!(
            expand("host=${A} port=5432"),
            Ok(("host=alpha port=5432".to_owned(), vec!["A".to_owned()]))
        );
        assert_eq!(
            expand("${A}${A}"),
            Ok(("alphaalpha".to_owned(), vec!["A".to_owned(), "A".to_owned()]))
        );
        // Only `${…}` is a reference, which is why a literal `$` costs nothing.
        assert_eq!(expand("$A"), Ok(("$A".to_owned(), Vec::new())));
        assert_eq!(expand("plain"), Ok(("plain".to_owned(), Vec::new())));

        for (value, named) in [
            ("${B}", "B"),
            ("${lower}", "lower"),
            ("${1A}", "1A"),
            (concat!("${A", ":-default}"), "A"),
            ("${}", "${}"),
            ("${B", "${"),
            // Confiture expands this one and leaves `${B` behind; here it is refused.
            ("${A} ${B", "${"),
            ("${NEST}", "NEST"),
        ] {
            let refusal = expand(value).expect_err(&format!("{value} is refused"));
            assert!(refusal.contains(named), "the refusal of {value} names {named}: {refusal}");
            assert!(
                refusal.contains("a.setting"),
                "the refusal of {value} names the setting: {refusal}"
            );
        }
    }

    #[test]
    fn a_lowercase_reference_is_refused_with_the_name_it_probably_meant() {
        // The mistake people make, and the one a refusal can shorten to one edit.
        let refusal = expanded("${webhook_url}", "confiture.notify_url", &vars())
            .expect_err("lowercase is not the strict form");
        assert!(refusal.contains("${WEBHOOK_URL}"), "{refusal}");
    }

    #[test]
    fn what_a_variable_holds_is_never_quoted_back() {
        // Every refusal here can be triggered by a value, and a value is the thing this crate
        // is careful with: a nested reference is the one case where the refusal has seen it.
        let vars = BTreeMap::from([(
            "SECRET".to_owned(),
            "postgresql://user:s3cret@host/db${".to_owned(),
        )]);
        let refusal = expanded("${SECRET}", "confiture.database_url", &vars)
            .expect_err("the value holds a reference of its own");
        assert!(refusal.contains("SECRET"), "the refusal names the variable: {refusal}");
        assert!(!refusal.contains("s3cret"), "and not what it holds: {refusal}");
    }

    #[test]
    fn a_name_is_the_strict_form_and_nothing_else() {
        assert!(is_variable_name("A"));
        assert!(is_variable_name("_A"));
        assert!(is_variable_name("MY_APP_DATABASE_URL"));
        assert!(is_variable_name("PG18"));
        assert!(!is_variable_name(""));
        assert!(!is_variable_name("1A"));
        assert!(!is_variable_name("lower"));
        assert!(!is_variable_name("MIXED_case"));
        assert!(!is_variable_name("WITH-DASH"));
        assert!(!is_variable_name("${A}"));
        assert!(!is_variable_name("postgresql://user@host/db"));
    }
}
