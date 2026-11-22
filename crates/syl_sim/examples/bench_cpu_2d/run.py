"""Run from the workspace root. Python 3.10+, C++20, Verilator 5.052."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

CASES = ["RoundRobinArbiter4", "SimpleDualPortRam", "SyncFifo-D8", "SyncFifo-D64",
         "LaneArray-N1", "LaneArray-N4", "LaneArray-N16", "LaneArray-N64", "Fault"]
BATCHES = [1, 16, 64, 256, 1024, 4096]
BIN = Path("target/release/examples/bench_cpu_2d").resolve()


def command(*args):
    return subprocess.check_output([str(a) for a in args], text=True)


def sample(args):
    start = time.perf_counter()
    raw = command(*args).splitlines()[-1].split()
    return dict(setup_s=float(raw[0]), hot_s=float(raw[1]), digest=int(raw[2]),
                failure=int(raw[3]), wall_s=time.perf_counter() - start)


def measure(args, fault=False):
    runs = [sample(args)]
    if not fault and runs[0]["hot_s"] < 0.25:
        runs.extend(sample(args) for _ in range(2))
    assert len({(r["digest"], r["failure"]) for r in runs}) == 1
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
    assert all(r["failure"] == (500 << 32) + 3073 for r in results.values()), results
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("docs/benchmarks/t3_cpu_2d.json"))
    parser.add_argument("--cases", nargs="+", default=CASES)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    subprocess.run(["cargo", "build", "--target-dir", "target", "--release", "-p", "syl_sim", "--example", "bench_cpu_2d"],
                   env=dict(os.environ, RUSTFLAGS="-Ctarget-cpu=native"), check=True)
    version = command("verilator", "--version").strip()
    assert "Verilator 5.052 " in version, version
    data = json.loads(args.output.read_text()) if args.resume and args.output.exists() else dict(
        cpu=command("lscpu"), system=platform.platform(), verilator=version,
        rustc=command("rustc", "-Vv"), cpp=command("g++", "--version").splitlines()[0],
        builds={}, rows=[], failure={})

    def persist():
        args.output.write_text(json.dumps(data, indent=2) + "\n")

    for case in args.cases:
        prior = {r["batch"] for r in data["rows"] if r["case"] == case}
        if prior == set(BATCHES) or (case == "Fault" and data["failure"]):
            continue
        with tempfile.TemporaryDirectory(prefix="syl-t3-") as tmp:
            folder = Path(tmp)
            if case == "Fault":
                data["failure"] = measure_fault(folder)
                persist()
                continue

            builds = {i: build(folder / str(i), case, i) for i in (1, 2)}
            data["builds"][case] = {i: seconds for i, (_, seconds) in builds.items()}
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
                assert all(r["failure"] == 2**64 - 1 for r in results.values()), results
                assert len({r["digest"] for r in results.values()}) == 1, (case, batch, results)
                data["rows"].append(dict(case=case, batch=batch, cycles=count, results=results))
                persist()


if __name__ == "__main__":
    main()
