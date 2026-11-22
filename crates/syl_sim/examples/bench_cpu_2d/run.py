"""Run from the workspace root. Python 3.10+, C++20, Verilator 5.052."""
import argparse
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

THROUGHPUT_CASES = ["RoundRobinArbiter4", "SimpleDualPortRam", "SyncFifo-D8", "SyncFifo-D64",
         "LaneArray-N1", "LaneArray-N4", "LaneArray-N16", "LaneArray-N64"]
CASES = THROUGHPUT_CASES + ["Fault"]
BATCHES = [1, 16, 64, 256, 1024, 4096]
# Golden outputs from the verified format-v2 matrix, in ascending batch size.
EXPECTED_THROUGHPUT_DIGESTS = {
    (case, batch): digest
    for case, digests in {
        "RoundRobinArbiter4": (
            5209017503618734443, 9218209677226779467, 11380903595388585226,
            17106762948914812018, 17071836774958863390, 12020996003381699917),
        "SimpleDualPortRam": (
            8650540608028339347, 9546040134826356013, 9653211605945073904,
            14597110560512966979, 9670094316022790332, 6607019259109491407),
        "SyncFifo-D8": (
            12861238034057911010, 334025169333683862, 2396965691768092965,
            2168975633948188960, 8111577841237920049, 13836929895425862317),
        "SyncFifo-D64": (
            9067166808205259551, 2800172720975498024, 1322656135809853445,
            16539323837819289271, 4899422867358870716, 141039909748334605),
        "LaneArray-N1": (
            13489606331283376550, 7605078613985149322, 14263058525684361543,
            11302983647163634405, 2916313884596197931, 3369134271052469832),
        "LaneArray-N4": (
            3692248856015827678, 1885580247445829542, 8150955254614456390,
            18222492794946700875, 1186930140080031722, 694183099661000295),
        "LaneArray-N16": (
            17360358802705982404, 6991395816702655219, 12172545905301647592,
            6874003765685272719, 9675898006659697059, 6327325067053878646),
        "LaneArray-N64": (
            3818117848507102531, 7158640249534982732, 6711939975407609069,
            16955472669682096980, 2967889634390082124, 10845028691391121441),
    }.items()
    for batch, digest in zip((1, 16, 64, 256, 1024, 4096), digests, strict=True)
}
TIMING_TOLERANCE_S = 1e-3
BIN = Path("target/release/examples/bench_cpu_2d").resolve()


def command(*args):
    return subprocess.check_output([str(a) for a in args], text=True)


def sample(args):
    start = time.perf_counter()
    raw = command(*args).splitlines()[-1].split()
    result = dict(setup_s=float(raw[0]), hot_s=float(raw[1]), digest=int(raw[2]),
                failure=int(raw[3]), wall_s=time.perf_counter() - start)
    if len(raw) == 5:
        result["checkpoint_bits"] = raw[4]
        result["checkpoint_cycle"] = 499
    return result


def measure(args, fault=False):
    runs = [sample(args)]
    if not fault and runs[0]["hot_s"] < 0.25:
        runs.extend(sample(args) for _ in range(2))
    if len({(r["digest"], r["failure"]) for r in runs}) != 1:
        raise ValueError(f"inconsistent digest or failure across repeats for {args}")
    result = dict(sorted(runs, key=lambda r: r["hot_s"])[len(runs) // 2])
    result["hot_samples_s"] = [r["hot_s"] for r in runs]
    return result


def build(folder, case, internal):
    folder.mkdir()
    start = time.perf_counter()
    top, *params = command(BIN, "emit", case, folder).split()
    # Preserve the width diagnostic for the constant DEPTH comparison.
    args = ["verilator", "--cc", "--exe", "--build", "-j", "16", "-O3", "-Wno-fatal",
            "--threads", str(internal), "--assert", "--top-module", top, "--prefix", "Vdut",
            "--Mdir", folder / "obj", "-CFLAGS", "-std=c++20 -O3 -march=native",
            "-MAKEFLAGS", "OPT_FAST=-O3", *params, folder / "design.sv", folder / "harness.cpp"]
    subprocess.run(args, stdout=subprocess.DEVNULL, check=True)
    return folder / "obj" / "Vdut", time.perf_counter() - start


def cycles(case, batch):
    lanes = int(case.split("-N")[1]) if "-N" in case else 1
    return max(16, min(1024, 1048576 // (lanes * batch)))


RUST_CONFIGS = [("scalar", 1), ("soa", 1), ("soa", 16), ("soa", 32)]


def host_workers(batch, internal):
    return sorted({1, min(batch, 16 // internal), min(batch, 32 // internal)})


def validate_configs(results, batch, case):
    expected = {f"{mode}{threads}" for mode, threads in RUST_CONFIGS}
    expected.update(f"v{i}x{w}" for i in (1, 2) for w in host_workers(batch, i))
    if not isinstance(results, dict) or set(results) != expected:
        raise ValueError(f"{case} B={batch} requires exactly {sorted(expected)}")


def valid_seconds(value):
    return type(value) in (float, int) and math.isfinite(value) and value > 0


def validate_timings(result, config, fault=False):
    samples = result.get("hot_samples_s")
    if not isinstance(samples, list) or not samples or not all(map(valid_seconds, samples)):
        raise ValueError(f"{config}: invalid hot timing samples")
    expected_count = 1 if fault or samples[0] >= 0.25 else 3
    if len(samples) != expected_count or result.get("hot_s") != statistics.median(samples):
        raise ValueError(f"{config}: hot time or repeat count disagrees with samples")
    if not (valid_seconds(result.get("setup_s")) and valid_seconds(result.get("wall_s"))):
        raise ValueError(f"{config}: invalid setup or process wall time")
    if result["wall_s"] + TIMING_TOLERANCE_S < result["setup_s"] + result["hot_s"]:
        raise ValueError(f"{config}: process wall time is shorter than setup plus hot time")
    if config.startswith("v") and not valid_seconds(result.get("compile_s")):
        raise ValueError(f"{config}: invalid compile time")
    if fault:
        cold = result.get("cold_wall_s")
        if not valid_seconds(cold):
            raise ValueError(f"{config}: invalid cold time")
        if config.startswith("v"):
            if cold + TIMING_TOLERANCE_S < result["compile_s"] + result["wall_s"]:
                raise ValueError(f"{config}: cold time is shorter than compile plus process time")
        elif cold != result["wall_s"]:
            raise ValueError(f"{config}: Rust cold time must equal process wall time")


def validate_row(row, builds):
    case, batch = row["case"], row["batch"]
    if (case not in THROUGHPUT_CASES or type(batch) is not int or batch not in BATCHES
            or row.get("cycles") != cycles(case, batch)):
        raise ValueError(f"invalid throughput point: {case} B={batch}")
    results = row["results"]
    validate_configs(results, batch, case)
    digest = results["scalar1"].get("digest")
    if type(digest) is not int or digest != EXPECTED_THROUGHPUT_DIGESTS[(case, batch)]:
        raise ValueError(f"{case} B={batch}: unexpected digest for this workload")
    for config, result in results.items():
        validate_timings(result, config)
        if (type(result.get("digest")) is not int or result["digest"] != digest
                or result.get("failure") != 2**64 - 1):
            raise ValueError(f"{case} B={batch} {config}: inconsistent digest or failure")
        if config.startswith("v") and result["compile_s"] != builds[config[1]]:
            raise ValueError(f"{case} B={batch} {config}: inconsistent build time")


def measure_fault(folder):
    batch, count = 4096, 500
    results = {}
    for mode, threads in RUST_CONFIGS:
        result = measure([BIN, mode, "Fault", batch, count, threads], fault=True)
        result["cold_wall_s"] = result["wall_s"]
        results[f"{mode}{threads}"] = result
    for internal in (1, 2):
        for workers in host_workers(batch, internal):
            config = f"v{internal}x{workers}"
            start = time.perf_counter()
            exe, compile_s = build(folder / config, "Fault", internal)
            result = measure([exe, batch, count, workers, internal], fault=True)
            result["cold_wall_s"] = time.perf_counter() - start
            result["compile_s"] = compile_s
            results[config] = result
    validate_fault(results, batch)
    return results


def splitmix64(x):
    mask = (1 << 64) - 1
    x = (x + 0x9e3779b97f4a7c15) & mask
    x = ((x ^ (x >> 30)) * 0xbf58476d1ce4e5b9) & mask
    x = ((x ^ (x >> 27)) * 0x94d049bb133111eb) & mask
    return x ^ (x >> 31)


def validate_fault(results, batch):
    if type(batch) is not int or batch != 4096:
        raise ValueError("Fault requires 4096 trajectories")
    validate_configs(results, batch, "Fault")
    # At edge 499 reset is inactive and the entire stored state is q := i.
    # Input i is physical port 3, limb 0 of the Fault fixture.
    bits = "".join(str(splitmix64(lane * 0x517cc1b727220a95
                                  ^ 499 * 0x5851f42d4c957f2d ^ (3 << 32)) & 1)
                   for lane in range(batch))
    digest = sum(splitmix64((lane << 1) | int(bit)) for lane, bit in enumerate(bits)) % (1 << 64)
    for config, result in results.items():
        if result.get("checkpoint_cycle") != 499 or result.get("checkpoint_bits") != bits:
            raise ValueError(f"Fault {config}: incorrect edge-499 checkpoint")
        if type(result.get("digest")) is not int or result["digest"] != digest:
            raise ValueError(f"Fault {config}: incorrect edge-499 digest")
        if type(result.get("failure")) is not int or result["failure"] != (500 << 32) + 3073:
            raise ValueError(f"Fault {config}: incorrect first failure")
        validate_timings(result, config, fault=True)


def completed_cases(data):
    if data.get("format_version") != 2:
        raise ValueError("incompatible timing protocol; use a fresh output or omit --resume")
    if not isinstance(data.get("rows"), list) or not isinstance(data.get("builds"), dict):
        raise ValueError("invalid cached throughput rows or builds")
    batches = {}
    for case, builds in data["builds"].items():
        if (case not in THROUGHPUT_CASES or set(builds) != {"1", "2"}
                or not all(map(valid_seconds, builds.values()))):
            raise ValueError(f"{case}: invalid Verilator builds")
    for row in data["rows"]:
        case, batch = row["case"], row["batch"]
        if case not in data["builds"]:
            raise ValueError(f"{case}: missing build times")
        validate_row(row, data["builds"][case])
        seen = batches.setdefault(case, set())
        if batch in seen:
            raise ValueError(f"{case} B={batch}: duplicate throughput point")
        seen.add(batch)
    complete = {case for case, seen in batches.items() if seen == set(BATCHES)}
    if data["failure"] != {}:
        validate_fault(data["failure"], 4096)
        complete.add("Fault")
    return complete


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("docs/benchmarks/t3_cpu_2d.json"))
    parser.add_argument("--cases", nargs="+", choices=CASES, default=CASES)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    data = None
    cases = list(dict.fromkeys(args.cases))
    if args.resume and args.output.exists():
        data = json.loads(args.output.read_text())
        complete = completed_cases(data)
        cases = [case for case in cases if case not in complete]
        if not cases:
            return

    subprocess.run(["cargo", "build", "--target-dir", "target", "--release", "-p", "syl_sim", "--example", "bench_cpu_2d"],
                   env=dict(os.environ, RUSTFLAGS="-Ctarget-cpu=native"), check=True)
    version = command("verilator", "--version").strip()
    if "Verilator 5.052 " not in version:
        raise RuntimeError(f"expected Verilator 5.052, found {version}")
    if data is None:
        data = dict(format_version=2, cpu=command("lscpu"), system=platform.platform(),
                    verilator=version, rustc=command("rustc", "-Vv"),
                    cpp=command("g++", "--version").splitlines()[0], builds={}, rows=[], failure={})

    def persist():
        args.output.write_text(json.dumps(data, indent=2) + "\n")

    for case in cases:
        with tempfile.TemporaryDirectory(prefix="syl-t3-") as tmp:
            folder = Path(tmp)
            if case == "Fault":
                data["failure"] = measure_fault(folder)
                persist()
                continue

            builds = {i: build(folder / str(i), case, i) for i in (1, 2)}
            data["builds"][case] = {str(i): seconds for i, (_, seconds) in builds.items()}
            # A restart builds fresh executables. Remeasure the whole partial case
            # so the reported cold build and every recorded hot sample refer to the same build.
            data["rows"] = [r for r in data["rows"] if r["case"] != case]
            for batch in BATCHES:
                count = cycles(case, batch)
                print(case, batch, count, flush=True)
                results = {}
                for mode, threads in RUST_CONFIGS:
                    results[f"{mode}{threads}"] = measure([BIN, mode, case, batch, count, threads])
                for internal, (exe, compile_s) in builds.items():
                    for workers in host_workers(batch, internal):
                        result = measure([exe, batch, count, workers, internal])
                        result["compile_s"] = compile_s
                        results[f"v{internal}x{workers}"] = result
                row = dict(case=case, batch=batch, cycles=count, results=results)
                validate_row(row, data["builds"][case])
                data["rows"].append(row)
                persist()


if __name__ == "__main__":
    main()
