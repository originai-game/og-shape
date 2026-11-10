//! Command-line arguments: one positional input path and `--flag value`
//! options, read by name.
//!
//! Entry: [`Args`].

use std::path::PathBuf;

use shape_tools::Import;

use crate::{Failure, Input};

/// The arguments after the command name.
pub struct Args {
    positional: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// Flags that take no value.
const SWITCHES: [&str; 8] = [
    "--uvs",
    "--no-edges",
    "--no-ground",
    "--weld-all",
    "--fuse",
    "--lock-border",
    "--intersections",
    "--z-up",
];

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

    /// The one input path: a program or a mesh file.
    pub fn input(&self) -> Result<Input, Failure> {
        match &self.positional[..] {
            [p] => Ok(Input::Path(PathBuf::from(p))),
            [] => Err(Failure::Usage("no input given".into())),
            _ => Err(Failure::Usage(format!(
                "one input, not {:?}",
                self.positional
            ))),
        }
    }

    /// How to read an OBJ file: `--unit-mm` and `--z-up`.
    pub fn import(&self) -> Result<Import, Failure> {
        Ok(Import {
            unit_mm: self.number("--unit-mm")?.unwrap_or(0.0),
            z_up: self.switch("--z-up"),
        })
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
        self.out_opt()?
            .ok_or_else(|| Failure::Usage("no output: -o <path>".into()))
    }

    /// The output path, if given.
    pub fn out_opt(&self) -> Result<Option<PathBuf>, Failure> {
        Ok(self.value(&["-o", "--out"])?.map(PathBuf::from))
    }
}

/// The options every command that reads a mesh takes.
pub const IMPORT_OPTIONS: [&str; 2] = ["--unit-mm", "--z-up"];
