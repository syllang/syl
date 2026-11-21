use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum DiagnosticsFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Args)]
pub(crate) struct CommonArgs {
    #[arg(long = "manifest", value_name = "PATH", global = true)]
    pub manifest: Option<PathBuf>,
    #[arg(long = "std-root", value_name = "PATH", global = true)]
    pub std_roots: Vec<PathBuf>,
    #[arg(long = "package-root", value_name = "PATH", global = true)]
    pub package_roots: Vec<PathBuf>,
    #[arg(long = "diagnostics-format", value_enum, default_value_t = DiagnosticsFormat::Human, global = true)]
    pub diagnostics_format: DiagnosticsFormat,
}

#[derive(Debug, Parser)]
#[command(
    name = "sylc",
    version,
    about = "Command-line compiler for Syl.",
    subcommand_precedence_over_arg = true
)]
pub(crate) struct SylcApp {
    #[command(subcommand)]
    pub command: Option<SylcCommand>,
    #[command(flatten)]
    pub common: CommonArgs,
    /// Output path for legacy direct invocation.
    #[arg(long = "out", value_name = "OUTPUT")]
    pub out_path: Option<PathBuf>,
    /// Input files or directories for legacy direct invocation.
    #[arg(value_name = "FILE_OR_DIR")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SylcCommand {
    /// Type-check and run elaboration DRC without emitting SystemVerilog.
    Check(CheckArgs),
    /// Check and emit SystemVerilog to stdout or --out.
    Emit(EmitArgs),
}
#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    #[arg(value_name = "FILE_OR_DIR")]
    pub inputs: Vec<PathBuf>,
}
#[derive(Debug, Args)]
pub(crate) struct EmitArgs {
    #[arg(long = "out", value_name = "OUTPUT")]
    pub out_path: Option<PathBuf>,
    #[arg(value_name = "FILE_OR_DIR")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunMode {
    Check,
    Emit { out_path: Option<PathBuf> },
}
#[derive(Debug)]
pub(crate) struct CliInvocation {
    pub common: CommonArgs,
    pub mode: RunMode,
    pub inputs: Vec<PathBuf>,
}
impl SylcApp {
    pub(crate) fn parse() -> Self {
        Self::try_parse_from(std::env::args_os()).unwrap_or_else(|error| error.exit())
    }

    pub(crate) fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
        let args = explicit_option_values(&args);
        let mut app = <Self as Parser>::try_parse_from(&args)?;
        // Clap propagates the deepest global value back to its parents, replacing
        // earlier occurrences even for Append. Recover only these validated,
        // repeatable options so roots on both sides retain declaration order.
        let roots = <Self as Parser>::try_parse_from(root_options(&args))?;
        app.common.std_roots = roots.common.std_roots;
        app.common.package_roots = roots.common.package_roots;
        Ok(app)
    }

    pub(crate) fn into_invocation(self) -> Result<CliInvocation, String> {
        let (mode, inputs) = match self.command {
            Some(SylcCommand::Check(args)) => {
                if self.out_path.is_some() {
                    return Err("--out is only valid for `emit`".into());
                }
                if !self.inputs.is_empty() {
                    return Err("unexpected positional arguments before `check`".into());
                }
                (RunMode::Check, args.inputs)
            }
            Some(SylcCommand::Emit(args)) => {
                if self.out_path.is_some() && args.out_path.is_some() {
                    return Err("--out was specified more than once".into());
                }
                if !self.inputs.is_empty() {
                    return Err("unexpected positional arguments before `emit`".into());
                }
                (
                    RunMode::Emit {
                        out_path: args.out_path.or(self.out_path),
                    },
                    args.inputs,
                )
            }
            None => (
                RunMode::Emit {
                    out_path: self.out_path,
                },
                self.inputs,
            ),
        };
        Ok(CliInvocation {
            common: self.common,
            mode,
            inputs,
        })
    }
}

fn explicit_option_values(args: &[OsString]) -> Vec<OsString> {
    let mut normalized: Vec<_> = args.first().cloned().into_iter().collect();
    let mut args = args.iter().skip(1).peekable();
    while let Some(arg) = args.next() {
        if arg == "--" {
            normalized.push(arg.clone());
            normalized.extend(args.cloned());
            break;
        }
        let mut arg = arg.clone();
        if matches!(
            arg.to_str(),
            Some("--manifest" | "--out" | "--std-root" | "--package-root" | "--diagnostics-format")
        ) && let Some(value) = args.peek()
            && (!value.as_encoded_bytes().starts_with(b"-") || *value == "-")
        {
            // Subcommand precedence must not turn an option value such as
            // `--manifest check` into a command. Equals syntax disambiguates
            // required values while leaving clap to validate missing values.
            arg.push("=");
            arg.push(args.next().expect("peeked option value"));
        }
        normalized.push(arg);
    }
    normalized
}

fn root_options(args: &[OsString]) -> Vec<OsString> {
    std::iter::once(OsString::from("sylc"))
        .chain(
            args.iter()
                .skip(1)
                .take_while(|arg| *arg != "--")
                .filter(|arg| {
                    arg.as_encoded_bytes().starts_with(b"--std-root=")
                        || arg.as_encoded_bytes().starts_with(b"--package-root=")
                })
                .cloned(),
        )
        .collect()
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
