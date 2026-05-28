#!/usr/bin/env python3
"""Validate Conventional Commit headers against repository scope policy."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from functools import lru_cache
from itertools import combinations
from pathlib import Path, PurePosixPath


HEADER_RE = re.compile(
    r"^(?P<type>[a-z][a-z0-9-]*)"
    r"\((?P<scope>[A-Za-z0-9._/+:-]+)\)"
    r"(?P<breaking>!)?: "
    r"(?P<subject>.+)$"
)


@dataclass(frozen=True)
class ScopeDefinition:
    name: str
    kind: str
    tier: str
    paths: tuple[str, ...]


class ValidationError(RuntimeError):
    def __init__(self, *lines: str) -> None:
        self.lines = lines
        super().__init__("\n".join(lines))


class ScopePolicy:
    def __init__(self, repo: Path, payload: dict[str, object]) -> None:
        self.repo = repo
        self.payload = payload
        self.forbidden_scopes = set(self.expect_list("forbidden_scopes"))
        scope_expression = self.expect_object("scope_expression")
        self.separator = self.expect_string(scope_expression, "separator")
        self.max_members = self.expect_int(scope_expression, "max_members")
        self.selection_policy = self.expect_object("selection_policy")
        self.atomic_payload = self.expect_object("atomic_scopes")
        self.composite_payload = self.expect_object("composite_scopes")
        self.definitions = self.build_definitions()

    def build_definitions(self) -> dict[str, ScopeDefinition]:
        definitions: dict[str, ScopeDefinition] = {}
        for name in self.atomic_payload:
            item = self.expect_nested_object(self.atomic_payload, name)
            definitions[name] = ScopeDefinition(
                name=name,
                kind=self.expect_string(item, "kind"),
                tier="atomic",
                paths=tuple(self.expect_list_from(item, "paths")),
            )
        for name in self.composite_payload:
            item = self.expect_nested_object(self.composite_payload, name)
            members = self.expect_list_from(item, "members")
            paths: list[str] = []
            for member in members:
                if member not in definitions:
                    raise ValidationError(
                        f"invalid scope policy: composite scope {name!r} references unknown member {member!r}"
                    )
                paths.extend(definitions[member].paths)
            definitions[name] = ScopeDefinition(
                name=name,
                kind=self.expect_string(item, "kind"),
                tier=self.expect_string(item, "tier"),
                paths=tuple(deduplicate(paths)),
            )
        return definitions

    def parse_scope_expression(self, scope: str) -> tuple[str, ...]:
        if scope in self.forbidden_scopes:
            raise ValidationError(
                f"invalid commit scope: {scope}",
                "This repository forbids broad escape-hatch scopes such as '.', 'workspace', or 'repo'.",
            )
        if scope.startswith("/") or "//" in scope:
            raise ValidationError(f"invalid commit scope: {scope}", "Scope must be relative.")
        parts = PurePosixPath(scope).parts
        if any(part == ".." for part in parts):
            raise ValidationError(f"invalid commit scope: {scope}", "Scope must not contain '..'.")
        members = tuple(scope.split(self.separator))
        if any(not member for member in members):
            raise ValidationError(
                f"invalid commit scope: {scope}",
                f"Scope expression members must be non-empty and separated by {self.separator!r}.",
            )
        if len(members) > self.max_members:
            raise ValidationError(
                f"invalid commit scope: {scope}",
                f"Scope expression may contain at most {self.max_members} members.",
            )
        if len(set(members)) != len(members):
            raise ValidationError(
                f"invalid commit scope: {scope}",
                "Scope expression must not repeat members.",
            )
        unknown = [member for member in members if member not in self.definitions]
        if unknown:
            raise ValidationError(
                f"invalid commit scope: {scope}",
                "Unknown scope members:",
                *[f"  - {member}" for member in unknown],
            )
        return members

    def covers_all(self, members: tuple[str, ...], changed_paths: list[str]) -> bool:
        if not changed_paths:
            return True
        allowed = self.covered_paths(members)
        return all(path_is_covered(path, allowed) for path in changed_paths)

    def covered_paths(self, members: tuple[str, ...]) -> tuple[str, ...]:
        paths: list[str] = []
        for member in members:
            paths.extend(self.definitions[member].paths)
        return tuple(deduplicate(paths))

    def validate_minimality(self, requested: tuple[str, ...], changed_paths: list[str]) -> None:
        if not changed_paths:
            return
        if not self.selection_policy.get("reject_broader_covering_scope_when_narrower_exists", True):
            return
        narrower = self.narrower_candidates(requested, changed_paths)
        if narrower:
            suggestions = [self.render_scope(candidate) for candidate in narrower]
            raise ValidationError(
                f"scope {self.render_scope(requested)!r} is broader than necessary for changed paths",
                "Use the narrowest covering scope.",
                "",
                "Suggested scopes:",
                *[f"  - {scope}" for scope in suggestions],
            )

    def narrower_candidates(
        self, requested: tuple[str, ...], changed_paths: list[str]
    ) -> list[tuple[str, ...]]:
        requested_set = set(requested)
        requested_score = self.preference_score(requested)
        candidates: list[tuple[str, ...]] = []
        for candidate in self.enumerate_covering_candidates(changed_paths):
            candidate_set = set(candidate)
            if candidate_set == requested_set and len(candidate) == len(requested):
                continue
            if self.preference_score(candidate) < requested_score:
                candidates.append(candidate)
        sorted_candidates = sorted(candidates, key=self.sort_key)
        if not sorted_candidates:
            return []
        best_score = self.sort_key(sorted_candidates[0])
        best_candidates = [candidate for candidate in sorted_candidates if self.sort_key(candidate) == best_score]
        return best_candidates[:5]

    def enumerate_covering_candidates(self, changed_paths: list[str]) -> list[tuple[str, ...]]:
        names = sorted(self.definitions)
        candidates: list[tuple[str, ...]] = []
        for size in range(1, self.max_members + 1):
            for combo in combinations(names, size):
                if self.covers_all(combo, changed_paths):
                    candidates.append(combo)
        return candidates

    def broadness(self, member: str) -> int:
        definition = self.definitions[member]
        if definition.tier == "atomic":
            return 0
        if definition.tier == "tight":
            return 1
        if definition.tier == "broad":
            return 2
        return 3

    def preference_score(self, members: tuple[str, ...]) -> tuple[int, int, int, int, str]:
        explicit_threshold = (
            self.max_members
            if self.selection_policy.get("prefer_explicit_combinations_when_member_count_lte", False)
            else 1
        )
        prefer_atomic = self.selection_policy.get("prefer_atomic_over_composite", True)
        all_atomic = all(self.broadness(member) == 0 for member in members)
        preferred_class = 0 if prefer_atomic and all_atomic and len(members) <= explicit_threshold else 1
        return (
            preferred_class,
            max(self.broadness(member) for member in members),
            sum(self.broadness(member) for member in members),
            len(members),
            self.render_scope(members),
        )

    def render_scope(self, members: tuple[str, ...]) -> str:
        return self.separator.join(members)

    def sort_key(self, candidate: tuple[str, ...]) -> tuple[int, int, int, int, str]:
        return self.preference_score(candidate)

    def expect_object(self, key: str) -> dict[str, object]:
        value = self.payload.get(key)
        if not isinstance(value, dict):
            raise ValidationError(f"invalid scope policy: {key} must be an object")
        return value

    @staticmethod
    def expect_nested_object(parent: dict[str, object], key: str) -> dict[str, object]:
        value = parent.get(key)
        if not isinstance(value, dict):
            raise ValidationError(f"invalid scope policy: {key} must be an object")
        return value

    def expect_list(self, key: str) -> list[str]:
        return self.expect_list_from(self.payload, key)

    @staticmethod
    def expect_list_from(parent: dict[str, object], key: str) -> list[str]:
        value = parent.get(key)
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            raise ValidationError(f"invalid scope policy: {key} must be a string list")
        return list(value)

    @staticmethod
    def expect_string(parent: dict[str, object], key: str) -> str:
        value = parent.get(key)
        if not isinstance(value, str):
            raise ValidationError(f"invalid scope policy: {key} must be a string")
        return value

    @staticmethod
    def expect_int(parent: dict[str, object], key: str) -> int:
        value = parent.get(key)
        if not isinstance(value, int):
            raise ValidationError(f"invalid scope policy: {key} must be an integer")
        return value


def deduplicate(items: list[str]) -> list[str]:
    return list(dict.fromkeys(items))


def path_is_covered(changed_path: str, allowed_paths: tuple[str, ...]) -> bool:
    for allowed in allowed_paths:
        if changed_path == allowed or changed_path.startswith(f"{allowed}/"):
            return True
    return False


def repo_root() -> Path:
    result = run_git("rev-parse", "--show-toplevel")
    return Path(result.stdout.strip())


def run_git(*args: str, cwd: Path | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=os.fspath(cwd) if cwd is not None else None,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def commit_paths(repo: Path, commit: str) -> list[str]:
    result = run_git("show", "--pretty=", "--name-only", "-z", commit, cwd=repo)
    if not result.stdout:
        return []
    return [path for path in result.stdout.split("\0") if path]


def range_paths(repo: Path, rev_range: str) -> list[str]:
    result = run_git("diff", "--name-only", "-z", rev_range, cwd=repo)
    if not result.stdout:
        return []
    return [path for path in result.stdout.split("\0") if path]


def staged_paths(repo: Path) -> list[str]:
    result = run_git("diff", "--cached", "--name-only", "-z", cwd=repo)
    if not result.stdout:
        return []
    return [path for path in result.stdout.split("\0") if path]


def first_line(message: str) -> str:
    return message.splitlines()[0] if message.splitlines() else ""


@lru_cache(maxsize=1)
def load_policy(repo: Path) -> ScopePolicy:
    path = repo / ".commit-scope.json"
    payload = json.loads(path.read_text(encoding="utf-8"))
    return ScopePolicy(repo, payload)


def validate_header(policy: ScopePolicy, subject: str, changed_paths: list[str]) -> None:
    match = HEADER_RE.fullmatch(subject)
    if match is None:
        raise ValidationError(
            "invalid commit message",
            "",
            "Expected Conventional Commit format with a required scope:",
            "  type(scope): subject",
            "  type(scope)!: subject",
            "",
            "Examples:",
            "  feat(core): add source spans",
            "  feat(core+docs): tighten completion contexts",
            "  chore(tooling): update repository hooks",
        )
    scope_text = match.group("scope")
    is_breaking = match.group("breaking") is not None
    members = policy.parse_scope_expression(scope_text)
    covered_paths = policy.covered_paths(members)
    in_scope_paths = [path for path in changed_paths if path_is_covered(path, covered_paths)]
    if is_breaking:
        if changed_paths and not in_scope_paths:
            raise ValidationError(
                f"breaking change scope {scope_text!r} must cover at least one changed path",
                "",
                "Changed paths do not match the declared breaking-change scope:",
                *[f"  - {path}" for path in changed_paths],
                "",
                "Use a scope that covers the primary breaking change or split unrelated work into separate commits.",
            )
        policy.validate_minimality(members, in_scope_paths)
        return
    if not policy.covers_all(members, changed_paths):
        uncovered = [path for path in changed_paths if not path_is_covered(path, covered_paths)]
        raise ValidationError(
            f"scope {scope_text!r} does not cover all changed paths",
            "",
            "Uncovered paths:",
            *[f"  - {path}" for path in uncovered],
            "",
            "Split unrelated changes into separate commits or use a valid covering scope expression.",
        )
    policy.validate_minimality(members, changed_paths)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)

    commit_msg = subparsers.add_parser("commit-msg")
    commit_msg.add_argument("message_file")

    subject = subparsers.add_parser("subject")
    subject.add_argument("message_file")
    subject.add_argument("--rev-range", required=True)

    header = subparsers.add_parser("header")
    header.add_argument("message_file")

    commit = subparsers.add_parser("commit")
    commit.add_argument("commit")

    parser.add_argument("--repo", default=None)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = Path(args.repo).resolve() if args.repo else repo_root()
    policy = load_policy(repo)
    try:
        if args.mode == "commit-msg":
            subject = first_line(Path(args.message_file).read_text(encoding="utf-8"))
            validate_header(policy, subject, staged_paths(repo))
            return 0
        if args.mode == "commit":
            subject = run_git("log", "--format=%s", "-n", "1", args.commit, cwd=repo).stdout.strip()
            validate_header(policy, subject, commit_paths(repo, args.commit))
            return 0
        if args.mode == "subject":
            subject = first_line(Path(args.message_file).read_text(encoding="utf-8"))
            validate_header(policy, subject, range_paths(repo, args.rev_range))
            return 0
        if args.mode == "header":
            subject = first_line(Path(args.message_file).read_text(encoding="utf-8"))
            validate_header(policy, subject, [])
            return 0
    except ValidationError as error:
        print("\n".join(error.lines), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
