use clap::Parser;
use std::env;
use std::fs;
use std::path::PathBuf;
use syl_emit::SystemVerilogBackend;
use syl_session::{AnalysisHost, ProjectConfig};

fn main() {
    if let Err(message) = SylcApp::parse().run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[derive(Debug, Parser)]
#[command(name = "sylc", version, about = "Command-line compiler for Syl.")]
struct SylcApp {
    #[arg(long = "out", value_name = "OUTPUT")]
    out_path: Option<PathBuf>,
    #[arg(long = "std-root", value_name = "PATH")]
    std_roots: Vec<PathBuf>,
    #[arg(value_name = "FILE_OR_DIR", required = true)]
    inputs: Vec<PathBuf>,
}

impl SylcApp {
    fn run(self) -> Result<(), String> {
        let mut host = AnalysisHost::with_config(self.project_config()?);
        let snapshot = host.load(&self.inputs).map_err(|err| err.to_string())?;
        if !snapshot.diagnostics().is_empty() {
            return Err(self.format_diagnostics(snapshot.diagnostics()));
        }
        let semantic_diagnostics = snapshot.semantic_diagnostics();
        if !semantic_diagnostics.is_empty() {
            return Err(self.format_diagnostics(&semantic_diagnostics));
        }
        let hwir = snapshot.hwir().ok_or_else(|| {
            "clean analysis snapshot did not produce a hardware graph".to_string()
        })?;
        let verilog = SystemVerilogBackend::new()
            .emit(hwir)
            .map_err(|err| err.to_string())?;
        if let Some(path) = self.out_path {
            fs::write(&path, verilog)
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        } else {
            print!("{verilog}");
        }
        Ok(())
    }

    fn project_config(&self) -> Result<ProjectConfig, String> {
        // CLI boundary: the current directory is an explicit user-selected workspace root.
        // Import resolution itself remains configured through ProjectConfig, not hidden in
        // the CLI.
        let cwd =
            env::current_dir().map_err(|err| format!("failed to read current directory: {err}"))?;
        let mut config = ProjectConfig::new().with_workspace_root(cwd);
        for root in &self.std_roots {
            config = config.with_std_root(root.clone());
        }
        Ok(config)
    }

    fn format_diagnostics(&self, diagnostics: &[syl_span::Diagnostic]) -> String {
        let mut message = "failed to compile project".to_string();
        for diagnostic in diagnostics {
            message.push_str(&format!("\n  {diagnostic}"));
        }
        message
    }
}
