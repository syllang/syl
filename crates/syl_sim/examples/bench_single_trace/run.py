"""Build the release example first. Run from the repository root."""
import argparse
import json
import os
import platform
import sys
import tempfile
from pathlib import Path
import subprocess
import time

CASES = ["RoundRobinArbiter4", "SimpleDualPortRam", "SyncFifo-D8", "SyncFifo-D64",
         "LaneArray-N1", "LaneArray-N4", "LaneArray-N16", "LaneArray-N64",
         "WideAluArray", "DeepFsmChain", "SharedRamArbiter", "FaultHuntPipeline"]
FAMILIES = {"LaneArray-N4", "LaneArray-N16", "LaneArray-N64", "WideAluArray"}
BIN = Path("target/release/examples/bench_single_trace").resolve()


def measure(args, cpu):
    if cpu is not None:
        args = ["taskset", "-c", str(cpu), *args]
    start = time.perf_counter()
    point = json.loads(subprocess.check_output(args, text=True))
    point["process_ms"] = (time.perf_counter() - start) * 1000
    return point


def run(case, engine, folder, cpu):
    build_ms = 0
    if engine == "verilator":
        start = time.perf_counter()
        top, *params = subprocess.check_output([BIN, "emit", case, folder], text=True).split()
        subprocess.run(["verilator", "--cc", "--exe", "--build", "-j", "8", "-O3",
                        "--threads", "1", "--assert", "-Wno-fatal", "--top-module", top,
                        "--prefix", "Vdut", "--Mdir", folder / "obj", "-CFLAGS",
                        "-std=c++20 -O3 -march=native", "-MAKEFLAGS", "OPT_FAST=-O3",
                        *params, folder / "design.sv", folder / "harness.cpp"],
                       stdout=subprocess.DEVNULL, check=True)
        build_ms = (time.perf_counter() - start) * 1000
        command = [folder / "obj" / "Vdut"]
    else:
        command = [BIN, engine, case]
    first = measure([*command, "1"], cpu)
    for cycles in [1024, 100000]:
        samples = [measure([*command, str(cycles)], cpu)]
        if samples[0]["hot_ms"] < 250:
            samples.extend(measure([*command, str(cycles)], cpu) for _ in range(2))
        if len({s["digest"] for s in samples}) != 1:
            raise ValueError(f"non-deterministic {case} {engine} K={cycles}")
        point = dict(sorted(samples, key=lambda s: s["hot_ms"])[len(samples) // 2])
        point["cold_ms"] += build_ms
        point.setdefault("compile_ms", build_ms)
        point.update(case=case, engine=engine, cycles=cycles,
                     hot_samples_ms=[s["hot_ms"] for s in samples],
                     first_cycle_digest=first["digest"],
                     first_cycle_ms=build_ms + first["process_ms"],
                     cold_plus_hot_ms=point["cold_ms"] + point["hot_ms"],
                     mcycles_s=cycles / point["hot_ms"] / 1000)
        yield point


def version(*args):
    return subprocess.check_output(args, text=True).splitlines()[0]


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--cpu", type=int)
    args = parser.parse_args()
    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        for case in CASES:
            folder = Path(tmp) / case
            folder.mkdir()
            expected = {}
            for engine in ["cpu-ref", "cpu-simd", "family", "cpu-jit", "verilator"]:
                if engine == "family" and case not in FAMILIES:
                    continue
                print(f"{case} {engine}", file=sys.stderr, flush=True)
                for row in run(case, engine, folder, args.cpu):
                    for cycles, digest in [(1, row["first_cycle_digest"]), (row["cycles"], row["digest"])]:
                        if expected.setdefault(cycles, digest) != digest:
                            raise ValueError(f"digest mismatch {case} {engine} K={cycles}")
                    rows.append(row)
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")), platform.processor())
    report = dict(cpu=cpu, cpu_affinity=args.cpu, os=platform.platform(), batch=1, seed=0,
                  rustc=version("rustc","--version"), rustflags=os.environ.get("RUSTFLAGS",""),
                  verilator=version("verilator","--version"), cxx=version("c++","--version"),
                  rows=rows)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
