#!/usr/bin/env python3
import pathlib
import subprocess
import tempfile


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]


def markdown_files():
    for path in WORKSPACE.rglob("*.md"):
        parts = set(path.relative_to(WORKSPACE).parts)
        if parts & {".git", ".tmp", "target"}:
            continue
        yield path


def syl_examples():
    yield from sorted((WORKSPACE / "examples").rglob("*.syl"))


def syl_blocks(path):
    in_block = False
    tag = ""
    lines = []
    start_line = 0
    for number, line in enumerate(path.read_text().splitlines(), start=1):
        stripped = line.strip()
        if stripped.startswith("```"):
            fence_tag = stripped[3:].strip().split()
            if not in_block:
                tag = fence_tag[0] if fence_tag else ""
                in_block = tag == "syl"
                lines = []
                start_line = number + 1
            else:
                if in_block:
                    yield start_line, "\n".join(lines) + "\n"
                in_block = False
                tag = ""
                lines = []
            continue
        if in_block:
            lines.append(line)


def main():
    with tempfile.TemporaryDirectory(prefix="syl-doc-snippets-") as temp:
        temp_dir = pathlib.Path(temp)
        paths = [str(path) for path in syl_examples()]
        for md_path in markdown_files():
            rel = md_path.relative_to(WORKSPACE)
            for start_line, source in syl_blocks(md_path):
                snippet = temp_dir / f"{rel.as_posix().replace('/', '__')}__L{start_line}.syl"
                snippet.write_text(source)
                paths.append(str(snippet))

        if not paths:
            return

        subprocess.run(
            [
                "cargo",
                "run",
                "--quiet",
                "-p",
                "syl_fuzz",
                "--bin",
                "parser_fuzz",
                "--",
                "--expect-clean",
                *paths,
            ],
            cwd=WORKSPACE,
            check=True,
        )


if __name__ == "__main__":
    main()
