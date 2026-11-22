use super::*;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant},
};

// This executable shim exercises the CLI's actual compiler/probe/runner
// process boundary. Device availability and compiler version are controlled
// independently of the CI host.
const COMPILER: &str = r#"#!/bin/sh
set -eu
if [ "$1" = --version ]; then
    echo version >> calls
    cat version
    exit
fi
while [ "$1" != -o ]; do src=$1; shift; done
dst=$2
if grep -q cudaGetDeviceProperties "$src"; then
    echo probe >> calls
    printf '#!/bin/sh\necho sm_75\n' > "$dst"
else
    echo kernel >> calls
    if [ -n "${HOLD_COMPILE:-}" ]; then
        echo "$dst" > ready
        exec sleep 60
    fi
    cat > "$dst" <<'RUNNER'
#!/bin/sh
printf '{"batch":%s,"cycles":%s,"elapsed_ms":1,"h2d_ms":0,"d2h_ms":0,"peak_vram_mib":0,"digest":0,"fail_batch":null,"fail_cycle":null,"fail_assert":null}\n' "$1" "$2"
RUNNER
fi
chmod +x "$dst"
"#;

fn project() -> TestProject {
    let p = TestProject::new();
    p.write("top.syl", CLEAN);
    p.write("version", "compiler A\n");
    let compiler = p.write("bin/nvcc", COMPILER);
    fs::set_permissions(compiler, fs::Permissions::from_mode(0o755)).unwrap();
    let shadow = p.write("shadow/nvcc", "non-executable file");
    fs::set_permissions(shadow, fs::Permissions::from_mode(0o644)).unwrap();
    p
}

fn command(p: &TestProject, backend: &str) -> Command {
    let path = std::env::join_paths(
        [p.0.join("shadow"), p.0.join("bin")]
            .into_iter()
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sylc"));
    cmd.current_dir(&p.0)
        .env("PATH", path)
        .env("SYL_CUDA_ARCH", "sm_75")
        .args([
            "sim",
            "top.syl",
            "--top",
            "Top",
            "--batch",
            "4096",
            "--cycles",
            "1000",
            "--backend",
            backend,
            "--gpu-cache-dir",
            "cache",
            "--diagnostics-format",
            "json",
        ]);
    cmd
}

fn calls(p: &TestProject) -> String {
    fs::read_to_string(p.0.join("calls")).unwrap()
}

#[test]
fn version_is_queried_through_executable_path_on_every_discovery() {
    let p = project();
    for (version, cache, expected) in [
        ("A", "miss", "version\nprobe\nkernel\n"),
        ("A", "hit", "version\n"),
        ("B", "miss", "version\nprobe\nkernel\n"),
        ("A", "hit", "version\n"),
    ] {
        p.write("version", version);
        p.write("calls", "");
        let out = command(&p, "gpu-cuda").output().unwrap();
        success(&out);
        assert!(String::from_utf8_lossy(&out.stdout).contains(&format!("cache={cache}")));
        assert_eq!(calls(&p), expected);
    }
}

#[test]
fn higher_target_is_rejected_or_routed_to_cpu_before_kernel_compilation() {
    let p = project();
    for arch in ["sm_90", "sm_100a"] {
        let explicit = command(&p, "gpu-cuda")
            .env("SYL_CUDA_ARCH", arch)
            .output()
            .unwrap();
        failure(&explicit);
        let message = report(&explicit)["diagnostics"][0].clone();
        assert_eq!(message["code"], "E_SIM");
        let text = message["message"].as_str().unwrap();
        assert!(
            text.contains(arch) && text.contains("incompatible") && text.contains("sm_75"),
            "{text}"
        );
        let auto = command(&p, "auto")
            .env("SYL_CUDA_ARCH", arch)
            .output()
            .unwrap();
        success(&auto);
        assert!(String::from_utf8_lossy(&auto.stdout).contains("backend=cpu-simd"));
    }
    assert!(!calls(&p).lines().any(|s| s == "kernel"));
}

struct Group(Child);
impl Group {
    fn signal(&self, group: bool) -> bool {
        let pid = self.0.id().to_string();
        let target = if group { format!("-{pid}") } else { pid };
        Command::new("kill")
            .args(["-TERM", "--", &target])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        self.signal(true);
        let _ = self.0.wait();
    }
}

#[test]
fn interrupting_compilation_reclaims_only_abandoned_staging_directories() {
    let p = project();
    let mut group = Group(
        command(&p, "gpu-cuda")
            .env("HOLD_COMPILE", "1")
            .process_group(0)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    let dir = loop {
        if let Ok(s) = fs::read_to_string(p.0.join("ready"))
            && let Some(parent) = Path::new(s.trim()).parent()
        {
            break p.0.join(parent);
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "compiler never reached staging"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(dir.join("owner.lock").is_file());
    assert!(group.signal(false));
    assert!(!group.0.wait().unwrap().success());
    // The orphaned compiler still holds this directory, even after sylc exited.
    success(&command(&p, "gpu-cuda").output().unwrap());
    assert!(dir.join("runner.cu").is_file());
    drop(group);
    let start = Instant::now();
    while dir.exists() {
        let hot = command(&p, "gpu-cuda").output().unwrap();
        success(&hot);
        assert!(String::from_utf8_lossy(&hot.stdout).contains("cache=hit"));
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "orphan was never reclaimed"
        );
    }
    assert!(fs::read_dir(p.0.join("cache")).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".compile-")
    }));
}
