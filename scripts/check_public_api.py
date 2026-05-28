#!/usr/bin/env python3
import argparse
import difflib
import json
import os
import pathlib
import subprocess
import sys


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
SNAPSHOT = WORKSPACE / "api" / "public-surface.txt"
CONSUMERS = WORKSPACE / "api" / "public-api-consumers.md"
CONSUMER_START = "<!-- public-api-consumers:start -->"
CONSUMER_END = "<!-- public-api-consumers:end -->"
SIGNATURE_GUARDS = [
    "syl_emit|variant|syl_emit::CompileError::InvalidHwir|{ report: syl_hw::HwValidationReport }",
    "syl_hw|field|syl_hw::ids::ObjectId::0|usize",
]


def run(command, **kwargs):
    try:
        return subprocess.run(command, check=True, text=True, **kwargs)
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error


def workspace_packages():
    output = run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=WORKSPACE,
        stdout=subprocess.PIPE,
    ).stdout
    metadata = json.loads(output)
    packages = []
    for package in metadata["packages"]:
        if any("lib" in target["kind"] for target in package["targets"]):
            packages.append(package["name"])
    return sorted(packages)


def rustdoc_json(package):
    env = os.environ.copy()
    env.setdefault("RUSTC_BOOTSTRAP", "1")
    command = [
        "cargo",
        "rustdoc",
        "-p",
        package,
        "--lib",
        "--all-features",
        "--quiet",
        "--",
        "-Z",
        "unstable-options",
        "--output-format",
        "json",
    ]
    try:
        subprocess.run(command, cwd=WORKSPACE, env=env, check=True)
    except subprocess.CalledProcessError as error:
        raise SystemExit(
            "failed to build rustdoc JSON public surface for "
            f"{package}; install a nightly toolchain or allow RUSTC_BOOTSTRAP=1"
        ) from error
    return json.loads((WORKSPACE / "target" / "doc" / f"{package}.json").read_text())


def inner_kind(item):
    return next(iter(item["inner"]))


def type_name(value):
    if "primitive" in value:
        return value["primitive"]
    if "generic" in value:
        return value["generic"]
    if "resolved_path" in value:
        path = value["resolved_path"]["path"]
        args = generic_args(value["resolved_path"].get("args"))
        return f"{path}{args}"
    if "borrowed_ref" in value:
        ref = value["borrowed_ref"]
        mutability = "mut " if ref.get("is_mutable") else ""
        return f"&{mutability}{type_name(ref['type'])}"
    if "raw_pointer" in value:
        ptr = value["raw_pointer"]
        mutability = "mut" if ptr.get("is_mutable") else "const"
        return f"*{mutability} {type_name(ptr['type'])}"
    if "tuple" in value:
        return "(" + ", ".join(type_name(item) for item in value["tuple"]) + ")"
    if "slice" in value:
        return f"[{type_name(value['slice'])}]"
    if "array" in value:
        array = value["array"]
        return "[{}; {}]".format(type_name(array["type"]), array["len"])
    if "impl_trait" in value:
        return "impl " + " + ".join(bound_name(bound) for bound in value["impl_trait"])
    if "dyn_trait" in value:
        traits = value["dyn_trait"]["traits"]
        return "dyn " + " + ".join(trait_bound_name(item["trait"]) for item in traits)
    if "qualified_path" in value:
        qualified = value["qualified_path"]
        return qualified["name"]
    return json.dumps(value, sort_keys=True)


def generic_args(args):
    if not args:
        return ""
    angle = args.get("angle_bracketed")
    if not angle:
        return ""
    rendered = []
    for arg in angle["args"]:
        if "type" in arg:
            rendered.append(type_name(arg["type"]))
        elif "const" in arg:
            rendered.append(str(arg["const"]))
        elif "lifetime" in arg:
            rendered.append(arg["lifetime"])
    return "<" + ", ".join(rendered) + ">" if rendered else ""


def bound_name(bound):
    if "trait_bound" in bound:
        return trait_bound_name(bound["trait_bound"]["trait"])
    if "lifetime" in bound:
        return bound["lifetime"]
    return json.dumps(bound, sort_keys=True)


def trait_bound_name(bound):
    return bound["path"] + generic_args(bound.get("args"))


def function_signature(function):
    sig = function["sig"]
    inputs = ", ".join(f"{name}: {type_name(value)}" for name, value in sig["inputs"])
    output = sig.get("output")
    rendered = f"fn({inputs})"
    if output is not None:
        rendered = f"{rendered} -> {type_name(output)}"
    return rendered


def item_path(data, item_id, fallback):
    path = data["paths"].get(str(item_id))
    if path:
        return "::".join(path["path"])
    return fallback


def item_type(data, item_id):
    item = data["index"][str(item_id)]
    return type_name(item["inner"][inner_kind(item)])


def field_type(data, field_id):
    if field_id is None:
        return "<stripped>"
    return item_type(data, field_id)


def struct_field_ids(struct_kind):
    if not isinstance(struct_kind, dict):
        return []
    if "plain" in struct_kind:
        return struct_kind["plain"].get("fields", [])
    if "tuple" in struct_kind:
        return struct_kind["tuple"]
    return []


def variant_signature(data, variant):
    kind = variant["inner"]["variant"]["kind"]
    if kind == "plain":
        return "unit"
    if not isinstance(kind, dict):
        return json.dumps(kind, sort_keys=True)
    if "tuple" in kind:
        payload = ", ".join(field_type(data, field_id) for field_id in kind["tuple"])
        return f"({payload})"
    if "struct" in kind:
        fields = []
        for field_id in kind["struct"].get("fields", []):
            field = data["index"][str(field_id)]
            fields.append(f"{field['name']}: {item_type(data, field_id)}")
        return "{ " + ", ".join(fields) + " }"
    return json.dumps(kind, sort_keys=True)


def owner_name(data, impl_for):
    if "resolved_path" in impl_for:
        item_id = impl_for["resolved_path"].get("id")
        path = data["paths"].get(str(item_id)) if item_id is not None else None
        if path:
            return "::".join(path["path"])
        return type_name({"resolved_path": impl_for["resolved_path"]})
    return type_name(impl_for)


def records_for_package(package):
    data = rustdoc_json(package)
    records = []
    for raw_id, item in data["index"].items():
        item_id = int(raw_id)
        kind = inner_kind(item)
        if kind == "impl" and item.get("crate_id") == 0:
            records.extend(impl_records(data, package, item))
            continue
        if item["visibility"] != "public" or item.get("crate_id") != 0:
            continue
        if kind == "use":
            use = item["inner"]["use"]
            glob = "glob" if use["is_glob"] else "named"
            records.append(
                f"{package}|use|{use['name']}|{glob}|source={use['source']}"
            )
        elif kind in {"struct", "enum", "trait", "function", "type_alias", "constant", "static"}:
            if str(item_id) not in data["paths"]:
                continue
            path = item_path(data, item_id, f"{package}::{item['name']}")
            records.append(f"{package}|{kind}|{path}|{item_signature(item)}")
            records.extend(child_records(data, package, path, item))
    return records


def item_signature(item):
    kind = inner_kind(item)
    inner = item["inner"][kind]
    if kind == "function":
        return function_signature(inner)
    if kind == "type_alias":
        target = inner.get("type")
        return type_name(target) if target is not None else "extern"
    if kind in {"constant", "static"}:
        return type_name(inner["type"])
    return "public"


def child_records(data, package, path, item):
    kind = inner_kind(item)
    inner = item["inner"][kind]
    records = []
    if kind == "struct":
        for field_id in struct_field_ids(inner["kind"]):
            if field_id is None:
                continue
            field = data["index"][str(field_id)]
            if field["visibility"] == "public":
                records.append(
                    f"{package}|field|{path}::{field['name']}|{item_type(data, field_id)}"
                )
    elif kind == "enum":
        for variant_id in inner["variants"]:
            variant = data["index"][str(variant_id)]
            records.append(
                f"{package}|variant|{path}::{variant['name']}|"
                f"{variant_signature(data, variant)}"
            )
    elif kind == "trait":
        for child_id in inner["items"]:
            child = data["index"][str(child_id)]
            child_kind = inner_kind(child)
            if child_kind == "function":
                records.append(
                    f"{package}|trait_method|{path}::{child['name']}|"
                    f"{function_signature(child['inner']['function'])}"
                )
            elif child_kind in {"assoc_type", "assoc_const"}:
                records.append(f"{package}|{child_kind}|{path}::{child['name']}|public")
    return records


def impl_records(data, package, item):
    impl = item["inner"]["impl"]
    if impl["trait"] is not None or impl["is_synthetic"] or impl["blanket_impl"] is not None:
        return []
    owner = owner_name(data, impl["for"])
    records = []
    for child_id in impl["items"]:
        child = data["index"][str(child_id)]
        if child["visibility"] != "public":
            continue
        child_kind = inner_kind(child)
        if child_kind == "function":
            records.append(
                f"{package}|method|{owner}::{child['name']}|"
                f"{function_signature(child['inner']['function'])}"
            )
        elif child_kind == "assoc_const":
            records.append(f"{package}|assoc_const|{owner}::{child['name']}|public")
    return records


def current_surface():
    records = []
    for package in workspace_packages():
        records.extend(records_for_package(package))
    return sorted(set(records))


def check_snapshot(records):
    expected = SNAPSHOT.read_text().splitlines() if SNAPSHOT.is_file() else []
    if records == expected:
        return
    diff = "\n".join(
        difflib.unified_diff(expected, records, fromfile=str(SNAPSHOT), tofile="current")
    )
    raise SystemExit(
        "public API surface changed; run scripts/check_public_api.py --bless "
        "and document consumers\n"
        + diff
    )


def consumer_descriptions():
    text = CONSUMERS.read_text() if CONSUMERS.is_file() else ""
    descriptions = {}
    current = None
    lines = []
    for line in text.splitlines():
        if line.startswith("## Item-Level Surface Consumers"):
            if current and lines:
                descriptions[current] = " ".join(item.strip() for item in lines if item.strip())
            return descriptions
        if line.startswith("## ") and not line.startswith("## Item-Level"):
            if current and lines:
                descriptions[current] = " ".join(item.strip() for item in lines if item.strip())
            current = line[3:].strip()
            lines = []
        elif current and not line.startswith("<!--") and not line.startswith("- `"):
            lines.append(line)
    if current and lines:
        descriptions[current] = " ".join(item.strip() for item in lines if item.strip())
    return descriptions


def validate_consumers(records):
    if not CONSUMERS.is_file():
        raise SystemExit(f"missing public API consumer policy: {CONSUMERS}")
    text = CONSUMERS.read_text()
    missing = [record for record in records if f"`{record}`" not in text]
    if missing:
        sample = "\n".join(missing[:20])
        raise SystemExit(
            "missing item-level public API consumer notes for surface lines:\n" + sample
        )


def validate_signature_guards(records):
    missing = [record for record in SIGNATURE_GUARDS if record not in records]
    if missing:
        raise SystemExit(
            "public API signature guard failed; expected precise payload/field records:\n"
            + "\n".join(missing)
        )
    if not SNAPSHOT.is_file():
        return
    snapshot = set(SNAPSHOT.read_text().splitlines())
    missing_snapshot = [record for record in SIGNATURE_GUARDS if record not in snapshot]
    if missing_snapshot:
        raise SystemExit(
            "public API snapshot signature guard failed; run "
            "scripts/check_public_api.py --bless after fixing extractor output:\n"
            + "\n".join(missing_snapshot)
        )


def write_consumers(records):
    descriptions = consumer_descriptions()
    lines = [
        "## Item-Level Surface Consumers",
        "",
        "Every line below mirrors one `api/public-surface.txt` line from rustdoc JSON. The",
        "right-hand text identifies the consumers that justify keeping that exact public item",
        "exported; adding or changing a surface line requires updating this section.",
        "",
        CONSUMER_START,
    ]
    for record in records:
        crate = record.split("|", 1)[0]
        description = descriptions.get(crate, "Repository-local public surface consumers.")
        lines.append(f"- `{record}` - {description}")
    lines.append(CONSUMER_END)
    lines.append("")

    existing = CONSUMERS.read_text() if CONSUMERS.is_file() else "# Public API Consumers\n\n"
    if CONSUMER_START in existing and CONSUMER_END in existing:
        prefix = existing.split("## Item-Level Surface Consumers", 1)[0].rstrip()
        CONSUMERS.write_text(prefix + "\n\n" + "\n".join(lines))
    else:
        CONSUMERS.write_text(existing.rstrip() + "\n\n" + "\n".join(lines))


def validate_no_glob_reexports(records):
    glob_records = [record for record in records if "|use|" in record and "|glob|" in record]
    if glob_records:
        raise SystemExit("public glob re-exports are forbidden:\n" + "\n".join(glob_records))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--bless", action="store_true")
    args = parser.parse_args()

    records = current_surface()
    validate_no_glob_reexports(records)
    if args.bless:
        SNAPSHOT.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT.write_text("\n".join(records) + "\n")
        write_consumers(records)
        validate_signature_guards(records)
    else:
        check_snapshot(records)
        validate_signature_guards(records)
        validate_consumers(records)


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(1)
