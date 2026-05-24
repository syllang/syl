use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use syl_span::SourceId;
use syl_syntax::SourceParser;

fn main() {
    if let Err(message) = ParserFuzzApp::from_env().run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct ParserFuzzApp {
    expect_clean: bool,
    inputs: Vec<PathBuf>,
}

impl ParserFuzzApp {
    fn from_env() -> Self {
        let mut expect_clean = false;
        let mut inputs = Vec::new();
        for arg in env::args().skip(1) {
            if arg == "--expect-clean" {
                expect_clean = true;
            } else {
                inputs.push(PathBuf::from(arg));
            }
        }
        Self {
            expect_clean,
            inputs,
        }
    }

    fn run(self) -> Result<(), String> {
        if self.inputs.is_empty() {
            let mut bytes = Vec::new();
            io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|err| format!("failed to read stdin fuzz input: {err}"))?;
            return self.parse_one(
                "<stdin>",
                &String::from_utf8_lossy(&bytes),
                SourceId::new(0),
            );
        }

        let mut source_id = 0usize;
        for input in self.expanded_inputs()? {
            let bytes = fs::read(&input)
                .map_err(|err| format!("failed to read fuzz input {}: {err}", input.display()))?;
            self.parse_one(
                &input.display().to_string(),
                &String::from_utf8_lossy(&bytes),
                SourceId::new(source_id),
            )?;
            source_id = source_id.saturating_add(1);
        }
        Ok(())
    }

    fn expanded_inputs(&self) -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        for input in &self.inputs {
            if input.is_dir() {
                collect_files(input, &mut paths)?;
            } else {
                paths.push(input.clone());
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn parse_one(&self, label: &str, source: &str, source_id: SourceId) -> Result<(), String> {
        let (output, syntax) = SourceParser::new_in(source, source_id).parse_file_with_lossless();
        let rebuilt = syntax.source_text();
        if rebuilt != source {
            return Err(format!(
                "{label}: lossless parser did not preserve source text"
            ));
        }
        if self.expect_clean && !output.diagnostics.is_empty() {
            let diagnostics = output
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    format!(
                        "{}:{}",
                        diagnostic.code.as_deref().unwrap_or("<missing-code>"),
                        diagnostic
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!("{label}: expected clean parse\n{diagnostics}"));
        }
        Ok(())
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read fuzz corpus dir {}: {err}", dir.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read dir entry in {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}
