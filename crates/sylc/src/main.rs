use std::env;
use std::fs;
use std::path::PathBuf;
use syl_emit::SystemVerilogBackend;
use syl_session::{AnalysisHost, ProjectConfig};

fn main() {
    if let Err(message) = SylcApp::from_env().run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

struct SylcApp {
    out_path: Option<String>,
    inputs: Vec<PathBuf>,
}

impl SylcApp {
    fn from_env() -> Self {
        let mut args = env::args().skip(1);
        let mut out_path = None;
        let mut inputs = Vec::new();
        while let Some(arg) = args.next() {
            if arg == "--out" {
                out_path = args.next();
            } else {
                inputs.push(PathBuf::from(arg));
            }
        }
        Self { out_path, inputs }
    }

    fn run(self) -> Result<(), String> {
        if self.inputs.is_empty() {
            return Err("usage: sylc [--out output.sv] <file-or-dir>...".to_string());
        }
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
            fs::write(&path, verilog).map_err(|err| format!("failed to write {path}: {err}"))?;
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
        Ok(ProjectConfig::new().with_workspace_root(cwd))
    }

    fn format_diagnostics(&self, diagnostics: &[syl_span::Diagnostic]) -> String {
        let mut message = "failed to compile project".to_string();
        for diagnostic in diagnostics {
            message.push_str(&format!("\n  {diagnostic}"));
        }
        message
    }
}
