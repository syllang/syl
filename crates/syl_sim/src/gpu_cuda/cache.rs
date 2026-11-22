//! One translation unit per executable. A content key includes the actual
//! compiler version and architecture, but excludes runtime launch arguments.
use super::{
    RunnerOptions, RunnerResult,
    build_dir::{self, BuildDir},
    compile_for, launch,
};
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum CudaCache<'a> {
    #[default]
    Default,
    Directory(&'a Path),
    Disabled,
}

fn cache_root(cache: CudaCache<'_>) -> PathBuf {
    if let CudaCache::Directory(path) = cache {
        return path.into();
    }
    env::var_os("SYL_CUDA_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(|p| PathBuf::from(p).join("syl/cuda")))
        .unwrap_or_else(|| env::temp_dir().join("syl-cuda-cache"))
}

impl CudaCache<'_> {
    pub(crate) fn has_entries(self) -> bool {
        !matches!(self, Self::Disabled)
            && cache_root(self)
                .read_dir()
                .is_ok_and(|mut it| it.next().is_some())
    }
}

fn fingerprint(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

fn valid_arch(arch: &str) -> bool {
    if arch == "native" {
        return true;
    }
    let Some(digits) = arch.strip_prefix("sm_") else {
        return false;
    };
    let digits = digits
        .strip_suffix(|c: char| c.is_ascii_lowercase())
        .unwrap_or(digits);
    (2..=3).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

pub(super) fn target_arch() -> Result<String, String> {
    if let Some(arch) = env::var_os("SYL_CUDA_ARCH") {
        let arch = arch.to_str().filter(|s| valid_arch(s)).ok_or(
            "invalid SYL_CUDA_ARCH (expected sm_ followed by two or three digits and an optional lowercase letter, or native)",
        )?;
        return Ok(arch.to_owned());
    }
    // CUDA device ordinals can be remapped, including by UUID. Let nvcc
    // inspect the visible device in that case.
    if env::var_os("CUDA_VISIBLE_DEVICES").is_some() {
        return Ok("native".into());
    }
    let arch = Command::new("nvidia-smi")
        .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let caps = String::from_utf8(o.stdout).ok()?;
            let mut caps = caps.lines().map(str::trim);
            let first = caps.next()?;
            if caps.any(|cap| cap != first) {
                return None;
            }
            let (major, minor) = first.split_once('.')?;
            if !major.bytes().all(|n| n.is_ascii_digit())
                || minor.len() != 1
                || !minor.bytes().all(|n| n.is_ascii_digit())
            {
                return None;
            }
            Some(format!("sm_{major}{minor}")).filter(|s| valid_arch(s))
        });
    Ok(arch.unwrap_or_else(|| "native".into()))
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::symlink_metadata(path)?;
        let owner = tempfile::tempfile()?.metadata()?.uid();
        if !meta.is_dir() || meta.uid() != owner || meta.mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "CUDA executable cache must be an owned directory without group or world write permission",
            ));
        }
    }
    Ok(())
}

struct Executable {
    path: PathBuf,
    cache_hit: bool,
    compile_ms: f64,
    _build_dir: Option<BuildDir>,
}

pub(crate) struct Toolchain {
    nvcc: PathBuf,
    arch: String,
    version: Vec<u8>,
    native_device: Option<String>,
    directory: PathBuf,
    temporary: Option<tempfile::TempDir>,
}

impl Toolchain {
    pub(crate) fn discover(cache: CudaCache<'_>) -> Result<Self, String> {
        let tmp = matches!(cache, CudaCache::Disabled)
            .then(tempfile::tempdir)
            .transpose()
            .map_err(|e| e.to_string())?;
        let directory = tmp
            .as_ref()
            .map_or_else(|| cache_root(cache), |d| d.path().into());
        private_directory(&directory)
            .map_err(|e| format!("CUDA cache {}: {e}", directory.display()))?;
        build_dir::reclaim(&directory)
            .map_err(|e| format!("inspect CUDA cache staging directories: {e}"))?;
        let arch = target_arch()?;
        let nvcc = compiler_path()?;
        let version = compiler_version(&nvcc)?;
        let mut toolchain = Self {
            nvcc,
            native_device: None,
            arch,
            version,
            directory,
            temporary: tmp,
        };
        if toolchain.arch == "native" {
            toolchain.native_device = Some(toolchain.device_arch()?);
        }
        Ok(toolchain)
    }
    pub(crate) fn probe(&self) -> Result<(), String> {
        if self.native_device.is_none() {
            let device = self.device_arch()?;
            if capability(&self.arch) > capability(&device) {
                return Err(format!(
                    "CUDA target {} is incompatible with device {device}: target compute capability exceeds device capability",
                    self.arch
                ));
            }
        }
        Ok(())
    }
    fn device_arch(&self) -> Result<String, String> {
        let probe = self.executable(include_str!("probe.cu"), None)?;
        let output = Command::new(&probe.path)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.code() == Some(77) {
            return Err("nvcc is installed but no usable CUDA device is available".into());
        }
        if !output.status.success() {
            return Err(format!("CUDA device probe failed: {}", output.status));
        }
        let arch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !valid_arch(&arch) || arch == "native" {
            return Err("invalid compute capability from CUDA device probe".into());
        }
        Ok(arch)
    }
    fn path(&self, source: &str, arch: Option<&str>) -> PathBuf {
        let device = arch.and(self.native_device.as_deref()).unwrap_or_default();
        self.directory.join(fingerprint(&[
            source.as_bytes(),
            &self.version,
            arch.unwrap_or("host").as_bytes(),
            device.as_bytes(),
        ]))
    }
    pub(crate) fn contains(&self, source: &str) -> bool {
        self.temporary.is_none() && self.path(source, Some(&self.arch)).is_file()
    }
    fn executable(&self, source: &str, arch: Option<&str>) -> Result<Executable, String> {
        let path = self.path(source, arch);
        if self.temporary.is_none() && path.is_file() {
            return Ok(Executable {
                path,
                cache_hit: true,
                compile_ms: 0.,
                _build_dir: None,
            });
        }
        let temp = BuildDir::create(&self.directory).map_err(|e| e.to_string())?;
        let binary = temp.path().join("runner");
        let start = Instant::now();
        let compiler = temp.compiler(&self.nvcc).map_err(|e| e.to_string())?;
        compile_for(compiler, source, &binary, false, arch)?;
        let (path, build_dir) = if self.temporary.is_some() {
            (binary, Some(temp))
        } else {
            fs::rename(binary, &path).map_err(|e| format!("publish CUDA executable: {e}"))?;
            (path, None)
        };
        Ok(Executable {
            path,
            cache_hit: false,
            compile_ms: start.elapsed().as_secs_f64() * 1000.,
            _build_dir: build_dir,
        })
    }
    pub(crate) fn run(
        &self,
        source: &str,
        options: RunnerOptions,
        seed: u64,
    ) -> Result<RunnerResult, String> {
        let binary = self.executable(source, Some(&self.arch))?;
        launch(
            &binary.path,
            options,
            seed,
            binary.cache_hit,
            binary.compile_ms,
        )
    }
}

// Called on validated, concrete sm_ targets.
fn capability(arch: &str) -> u16 {
    arch[3..]
        .trim_end_matches(|c: char| c.is_ascii_lowercase())
        .parse()
        .expect("validated CUDA capability")
}

pub(super) fn compiler_path() -> Result<PathBuf, String> {
    let name = format!("nvcc{}", env::consts::EXE_SUFFIX);
    let path = env::var_os("PATH").ok_or("nvcc is unavailable")?;
    env::split_paths(&path)
        .map(|dir| dir.join(&name))
        .find_map(|candidate| {
            let meta = candidate.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 == 0 {
                    return None;
                }
            }
            candidate.canonicalize().ok()
        })
        .ok_or_else(|| "nvcc is unavailable".into())
}

pub(super) fn compiler_version(nvcc: &Path) -> Result<Vec<u8>, String> {
    // A stable wrapper can select a different underlying compiler on each run.
    let result = Command::new(nvcc).arg("--version").output().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            "nvcc is unavailable".to_string()
        } else {
            format!("run nvcc: {e}")
        }
    })?;
    if !result.status.success() {
        return Err("nvcc --version failed".into());
    }
    Ok(result.stdout)
}

/// Probe the compiler and the CUDA runtime in a child process.
pub fn probe_cuda() -> Result<(), String> {
    Toolchain::discover(CudaCache::Default)?.probe()
}
