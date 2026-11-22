use std::{
    env, fs,
    io::ErrorKind,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run(command: &mut Command, stage: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{stage}: could not launch {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{stage} failed ({command:?}, {})\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub struct SimulationDir(pub PathBuf);

impl SimulationDir {
    pub fn new(top: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("syl-sim-{top}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("simulation scratch directory must be creatable");
        Self(path)
    }
}

impl Drop for SimulationDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn tool_available(tool: &str) -> bool {
    let version = if matches!(tool, "iverilog" | "vvp") {
        "-V"
    } else {
        "--version"
    };
    match Command::new(tool).arg(version).output() {
        Ok(output) => {
            assert!(
                output.status.success(),
                "{tool} {version} failed: {output:?}"
            );
            true
        }
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => panic!("could not probe {tool}: {error}"),
    }
}
