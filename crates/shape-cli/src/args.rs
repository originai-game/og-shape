//! Command-line arguments: one positional program path and `--flag value`
//! options, read by name.

use std::path::{Path, PathBuf};

use shape_model::Model;

/// Why a command did not run.
pub enum Failure {
    /// The command line was wrong.
    Usage(String),
    /// The command ran and failed.
    Failed(String),
}

/// The arguments after the command name.
pub struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// Flags that take no value.
const SWITCHES: [&str; 2] = ["--no-edges", "--no-ground"];

impl Args {
    /// Splits `-o x`, `--name value` and switches from positionals.
    pub fn new(argv: &[String]) -> Args {
        let mut positional = Vec::new();
        let mut options = Vec::new();
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            if a.starts_with('-') && a.len() > 1 {
                if SWITCHES.contains(&a.as_str()) {
                    options.push((a.clone(), None));
                } else {
                    options.push((a.clone(), argv.get(i + 1).cloned()));
                    i += 1;
                }
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        Args {
            positional,
            options,
        }
    }

    /// The one program path.
    pub fn program(&self) -> Result<PathBuf, Failure> {
        match &self.positional[..] {
            [p] => Ok(PathBuf::from(p)),
            [] => Err(Failure::Usage("no program given".into())),
            _ => Err(Failure::Usage(format!(
                "one program, not {:?}",
                self.positional
            ))),
        }
    }

    /// The value of `--name` (or its short form), if given.
    pub fn value(&self, names: &[&str]) -> Result<Option<&str>, Failure> {
        match self
            .options
            .iter()
            .find(|(k, _)| names.contains(&k.as_str()))
        {
            None => Ok(None),
            Some((k, None)) => Err(Failure::Usage(format!("{k} needs a value"))),
            Some((_, Some(v))) => Ok(Some(v)),
        }
    }

    /// Whether a switch is given.
    pub fn switch(&self, name: &str) -> bool {
        self.options.iter().any(|(k, _)| k == name)
    }

    /// A number option.
    pub fn number<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, Failure> {
        self.value(&[name])?
            .map(|v| {
                v.parse()
                    .map_err(|_| Failure::Usage(format!("{name}: not a number: {v:?}")))
            })
            .transpose()
    }

    /// Fails on an option the command does not take.
    pub fn only(&self, known: &[&str]) -> Result<(), Failure> {
        for (k, _) in &self.options {
            if !known.contains(&k.as_str()) {
                return Err(Failure::Usage(format!("unknown option {k}")));
            }
        }
        Ok(())
    }

    /// The output path (`-o` / `--out`).
    pub fn out(&self) -> Result<PathBuf, Failure> {
        self.value(&["-o", "--out"])?
            .map(PathBuf::from)
            .ok_or_else(|| Failure::Usage("no output: -o <path>".into()))
    }
}

/// Reads and replays a program.
pub fn load(path: &Path) -> Result<Model, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
    shape_model::program::run(&text)
        .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
}

/// Writes bytes to a file.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    std::fs::write(path, bytes).map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
}

/// A model error as a failure.
pub fn failed(e: impl std::fmt::Display) -> Failure {
    Failure::Failed(e.to_string())
}
