mod cli;
mod diagnostics;
mod sim;

use cli::{CliInvocation, RunMode, SylcApp};
use diagnostics::{DiagnosticRenderer, project_error};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use syl_emit::{EmitOptions, SourceLocation, SourceLocationLookup, SystemVerilogBackend};
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
    if let RunMode::Sim(args) = invocation.mode {
        let params = sim::parameters(&args.params)?;
        let design =
            syl_sim::SimDesign::from_hwir(hwir, &args.top, &params).map_err(sim::design_error)?;
        return sim::run(&design, &args, &mut report.diagnostics);
    }
    let RunMode::Emit(output) = invocation.mode else {
        return Ok(());
    };
    let backend = SystemVerilogBackend::new();
    let options = EmitOptions::new().with_source_comments(output.source_comments);
    let lookup = SourceLookup(&report.source_map);
    if let Some(dir) = output.out_dir {
        let bundle = backend
            .emit_files_with_options(hwir, &options, Some(&lookup))
            .map_err(|err| cli_error("E_EMIT", err.to_string()))?;
        let list = output.filelist.unwrap_or_else(|| dir.join("filelist.f"));
        fs::create_dir_all(&dir).map_err(|err| {
            cli_error("E_IO", format!("failed to create {}: {err}", dir.display()))
        })?;
        let directory = fs::canonicalize(&dir).map_err(project_error_io)?;
        let list_parent = list
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let list_dir = fs::canonicalize(list_parent).map_err(project_error_io)?;
        let list_target = output_target(&list).map_err(project_error_io)?;
        for file in bundle.files() {
            if output_target(&directory.join(file.file_name())).map_err(project_error_io)?
                == list_target
            {
                return Err(cli_error(
                    "E_CLI",
                    "filelist path collides with a generated module",
                ));
            }
        }
        let mut filelist = String::new();
        for file in bundle.files() {
            write_output(&dir.join(file.file_name()), file.content())?;
            if list_dir != directory {
                let path = directory.join(file.file_name());
                let entry = path.to_string_lossy();
                if entry.contains(char::is_whitespace) {
                    filelist.push('"');
                    filelist.push_str(&entry);
                    filelist.push('"');
                } else {
                    filelist.push_str(&entry);
                }
                filelist.push('\n');
            }
        }
        return write_output(
            &list,
            if list_dir == directory {
                bundle.filelist()
            } else {
                &filelist
            },
        );
    }
    let verilog = backend
        .emit_with_options(hwir, &options, Some(&lookup))
        .map_err(|err| cli_error("E_EMIT", err.to_string()))?;
    if let Some(path) = output.out_path {
        write_output(&path, &verilog)?;
    } else {
        io::stdout()
            .lock()
            .write_all(verilog.as_bytes())
            .map_err(|err| cli_error("E_IO", format!("failed to write stdout: {err}")))?;
    }
    Ok(())
}

struct SourceLookup<'a>(&'a SourceMap);
impl SourceLocationLookup for SourceLookup<'_> {
    fn lookup(&self, index: usize, offset: usize) -> Option<SourceLocation> {
        let id = syl_span::SourceId::new(index);
        let file = self.0.file(id)?;
        let position = file.utf16_range(Span::new_in(id, offset, offset))?.start;
        Some(SourceLocation::new(file.uri(), position.line + 1))
    }
}

/// Resolve an output destination even if its final file has yet to be created.
fn output_target(path: &Path) -> io::Result<PathBuf> {
    let mut destination = path.to_owned();
    loop {
        match fs::canonicalize(&destination) {
            Ok(target) => return Ok(target),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = fs::canonicalize(parent)?;
        match fs::read_link(&destination) {
            Ok(target) => destination = parent.join(target),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let name = destination.file_name().ok_or(err)?;
                return Ok(parent.join(name));
            }
            Err(err) => return Err(err),
        }
    }
}

fn project_error_io(err: io::Error) -> Box<Diagnostic> {
    cli_error("E_IO", err.to_string())
}
fn write_output(path: &Path, contents: &str) -> Result<(), Box<Diagnostic>> {
    fs::write(path, contents)
        .map_err(|err| cli_error("E_IO", format!("failed to write {}: {err}", path.display())))
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
