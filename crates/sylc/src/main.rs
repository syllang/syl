mod cli;
mod diagnostics;

use cli::{CliInvocation, RunMode, SylcApp};
use diagnostics::{DiagnosticRenderer, project_error};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use syl_emit::SystemVerilogBackend;
use syl_session::{AnalysisHost, FsVfs, ProjectConfig, ProjectManifest, Vfs};
use syl_span::{Diagnostic, DiagnosticSeverity, SourceMap, Span};

fn main() -> ExitCode {
    let app = SylcApp::parse();
    let format = app.common.diagnostics_format;
    let cwd = env::current_dir();
    let display_base = cwd.as_deref().unwrap_or(Path::new("."));
    let mut report = Report::default();
    let result = app
        .into_invocation()
        .map_err(|err| cli_error("E_CLI", err))
        .and_then(|invocation| {
            let cwd = cwd.as_ref().map_err(|err| {
                cli_error("E_IO", format!("failed to read current directory: {err}"))
            })?;
            run(invocation, cwd, &mut report)
        });
    if let Err(diagnostic) = result {
        report.diagnostics.push(*diagnostic);
    }
    let renderer = DiagnosticRenderer::new(&report.source_map, display_base);
    let output = renderer.render(format, &report.diagnostics);
    let write_failed = io::stderr().lock().write_all(output.as_bytes()).is_err();
    if report.has_errors() || write_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Default)]
struct Report {
    source_map: SourceMap,
    diagnostics: Vec<Diagnostic>,
}
impl Report {
    fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    }
}

fn run(invocation: CliInvocation, cwd: &Path, report: &mut Report) -> Result<(), Box<Diagnostic>> {
    let manifest = resolve_manifest(&invocation, cwd)?;
    let mut config = manifest.as_ref().map_or_else(
        || ProjectConfig::new().with_workspace_root(cwd.into()),
        ProjectManifest::to_project_config,
    );
    let std_roots = merged_roots(&invocation.common.std_roots, config.std_roots(), cwd)?;
    let package_roots = merged_roots(
        &invocation.common.package_roots,
        config.package_roots(),
        cwd,
    )?;
    config = ProjectConfig::builder()
        .workspace_roots(config.workspace_roots().to_vec())
        .std_roots(std_roots)
        .package_roots(package_roots)
        .build();
    let inputs = if !invocation.inputs.is_empty() {
        invocation.inputs
    } else if let Some(manifest) = &manifest {
        manifest.workspace_roots().to_vec()
    } else {
        Vec::new()
    };
    if inputs.is_empty() {
        return Err(cli_error(
            "E_CLI",
            "no input files specified and no workspace roots available; pass FILE_OR_DIR or configure syl.toml",
        ));
    }
    let mut host = AnalysisHost::with_config(config);
    if let Some(manifest) = &manifest {
        host.load_manifest_opaque_summaries(manifest)
            .map_err(project_error)?;
    }
    let snapshot = host.load(&inputs).map_err(project_error)?;
    report.source_map = snapshot.source_map().clone();
    report.diagnostics.extend_from_slice(snapshot.diagnostics());
    if !report.has_errors() {
        report.diagnostics.extend(snapshot.semantic_diagnostics());
    }
    if report.has_errors() || matches!(invocation.mode, RunMode::Check) {
        return Ok(());
    }
    let hwir = snapshot.hwir().ok_or_else(|| {
        cli_error(
            "E_EMIT",
            "clean analysis snapshot did not produce a hardware graph",
        )
    })?;
    let verilog = SystemVerilogBackend::new()
        .emit(hwir)
        .map_err(|err| cli_error("E_EMIT", err.to_string()))?;
    if let RunMode::Emit {
        out_path: Some(path),
    } = invocation.mode
    {
        fs::write(&path, verilog).map_err(|err| {
            cli_error("E_IO", format!("failed to write {}: {err}", path.display()))
        })?;
    } else {
        io::stdout()
            .lock()
            .write_all(verilog.as_bytes())
            .map_err(|err| cli_error("E_IO", format!("failed to write stdout: {err}")))?;
    }
    Ok(())
}

fn resolve_manifest(
    invocation: &CliInvocation,
    cwd: &Path,
) -> Result<Option<ProjectManifest>, Box<Diagnostic>> {
    if let Some(path) = &invocation.common.manifest {
        return ProjectManifest::load(path).map(Some).map_err(project_error);
    }
    if invocation.inputs.is_empty() {
        return ProjectManifest::discover_and_load(cwd).map_err(project_error);
    }
    let mut manifests = Vec::new();
    for input in &invocation.inputs {
        if let Some(path) = ProjectManifest::discover(input).map_err(project_error)?
            && !manifests.contains(&path)
        {
            manifests.push(path);
        }
    }
    if manifests.len() > 1 {
        return Err(cli_error(
            "E_CLI",
            format!(
                "inputs span multiple syl.toml manifests ({}); pass --manifest explicitly",
                manifests
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    manifests
        .first()
        .map(|path| ProjectManifest::load(path))
        .transpose()
        .map_err(project_error)
}

fn merged_roots(
    cli: &[PathBuf],
    manifest: &[PathBuf],
    cwd: &Path,
) -> Result<Vec<PathBuf>, Box<Diagnostic>> {
    let mut roots = Vec::new();
    let vfs = FsVfs::default();
    for root in cli.iter().chain(manifest) {
        let path = cwd.join(root);
        let path = if vfs.exists(&path) {
            vfs.canonicalize(&path).map_err(project_error)?
        } else {
            path
        };
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    Ok(roots)
}

fn cli_error(code: &str, message: impl Into<String>) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(Span::default(), message)
            .with_code(code)
            .with_source("sylc"),
    )
}
