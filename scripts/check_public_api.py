#!/usr/bin/env python3
import argparse
import difflib
import pathlib
import re
import sys


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
SNAPSHOT = WORKSPACE / "api" / "public-surface.txt"
CONSUMERS = WORKSPACE / "api" / "public-api-consumers.md"

ITEM_RE = re.compile(
    r"^\s*pub\s+(?!(?:crate|super|self|in)\b)"
    r"(?P<kind>struct|enum|trait|type|fn|const|static|mod)\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
USE_RE = re.compile(r"^\s*pub\s+use\s+(?P<target>[^;]+);")
IMPL_RE = re.compile(r"^\s*impl(?:<[^>]+>)?\s+(?P<target>[A-Za-z_][A-Za-z0-9_:<>', ]*)\s*\{")
METHOD_RE = re.compile(r"^\s*pub\s+(?!(?:crate|super|self|in)\b)fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)")


def strip_line_comment(line):
    return line.split("//", 1)[0]


def module_path(crate_src, path):
    rel = path.relative_to(crate_src)
    if rel.name == "lib.rs":
        return "lib"
    without_suffix = rel.with_suffix("")
    return "::".join(without_suffix.parts)


def public_items_for_file(crate_name, crate_src, path):
    records = []
    depth = 0
    impl_target = None
    impl_depth = None
    pending_use = None
    pending_use_line = None
    for number, raw in enumerate(path.read_text().splitlines(), start=1):
        line = strip_line_comment(raw)
        before_depth = depth

        if pending_use is not None:
            pending_use = f"{pending_use} {' '.join(line.strip().split())}"
            if ";" in line:
                if "*" in pending_use:
                    records.append(
                        f"{crate_name}|forbidden_glob_use|{module_path(crate_src, path)}|"
                        f"{pending_use_line}|{pending_use.strip()}"
                    )
                if match := USE_RE.match(pending_use):
                    target = " ".join(match.group("target").split())
                    records.append(
                        f"{crate_name}|use|{target}|"
                        f"{module_path(crate_src, path)}:{pending_use_line}"
                    )
                pending_use = None
                pending_use_line = None
            depth += line.count("{") - line.count("}")
            continue

        if before_depth == 0 and line.lstrip().startswith("pub use") and ";" not in line:
            pending_use = " ".join(line.strip().split())
            pending_use_line = number
            depth += line.count("{") - line.count("}")
            continue

        if before_depth == 0 and "pub use" in line and "*" in line:
            records.append(
                f"{crate_name}|forbidden_glob_use|{module_path(crate_src, path)}|"
                f"{number}|{line.strip()}"
            )

        if before_depth == 0:
            if match := USE_RE.match(line):
                target = " ".join(match.group("target").split())
                records.append(f"{crate_name}|use|{target}|{module_path(crate_src, path)}:{number}")
            elif match := ITEM_RE.match(line):
                records.append(
                    f"{crate_name}|{match.group('kind')}|{match.group('name')}|"
                    f"{module_path(crate_src, path)}:{number}"
                )
            if match := IMPL_RE.match(line):
                impl_target = " ".join(match.group("target").split())
                impl_depth = before_depth + line.count("{") - line.count("}")
        elif impl_target is not None and before_depth == impl_depth:
            if match := METHOD_RE.match(line):
                records.append(
                    f"{crate_name}|method|{impl_target}::{match.group('name')}|"
                    f"{module_path(crate_src, path)}:{number}"
                )

        depth += line.count("{") - line.count("}")
        if impl_target is not None and depth < impl_depth:
            impl_target = None
            impl_depth = None
    return records


def current_surface():
    records = []
    for manifest in sorted((WORKSPACE / "crates").glob("*/Cargo.toml")):
        crate_name = manifest.parent.name
        crate_src = manifest.parent / "src"
        if not crate_src.is_dir():
            continue
        for path in sorted(crate_src.rglob("*.rs")):
            records.extend(public_items_for_file(crate_name, crate_src, path))
    return sorted(records)


def consumer_sections():
    if not CONSUMERS.is_file():
        return set()
    return {
        line[3:].strip()
        for line in CONSUMERS.read_text().splitlines()
        if line.startswith("## ")
    }


def validate_consumers(records):
    crates = {record.split("|", 1)[0] for record in records}
    sections = consumer_sections()
    missing = sorted(crate for crate in crates if crate not in sections)
    if missing:
        raise SystemExit(
            "missing public API consumer sections for crates: " + ", ".join(missing)
        )


def check_snapshot(records):
    expected = SNAPSHOT.read_text().splitlines() if SNAPSHOT.is_file() else []
    if records == expected:
        return
    diff = "\n".join(
        difflib.unified_diff(expected, records, fromfile=str(SNAPSHOT), tofile="current")
    )
    raise SystemExit(
        "public API surface changed; update api/public-surface.txt and document consumers\n"
        + diff
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--bless", action="store_true")
    args = parser.parse_args()

    records = current_surface()
    validate_consumers(records)
    if args.bless:
        SNAPSHOT.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT.write_text("\n".join(records) + "\n")
    else:
        check_snapshot(records)


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(1)
