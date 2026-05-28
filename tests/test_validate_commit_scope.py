from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "validate_commit_scope.py"
SPEC = importlib.util.spec_from_file_location("validate_commit_scope", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"failed to load module from {MODULE_PATH}")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def make_policy() -> object:
    return MODULE.ScopePolicy(
        Path("."),
        {
            "forbidden_scopes": [".", "workspace", "repo"],
            "scope_expression": {
                "separator": "+",
                "max_members": 3,
            },
            "selection_policy": {
                "prefer_atomic_over_composite": True,
                "prefer_explicit_combinations_when_member_count_lte": 3,
                "reject_broader_covering_scope_when_narrower_exists": True,
            },
            "atomic_scopes": {
                "core": {
                    "kind": "path",
                    "paths": ["src/core"],
                },
                "docs": {
                    "kind": "path",
                    "paths": ["docs"],
                },
            },
            "composite_scopes": {
                "product": {
                    "kind": "domain",
                    "tier": "tight",
                    "members": ["core", "docs"],
                }
            },
        },
    )


class ValidateCommitScopeTests(unittest.TestCase):
    def test_non_breaking_commit_rejects_out_of_scope_paths(self) -> None:
        policy = make_policy()

        with self.assertRaises(MODULE.ValidationError):
            MODULE.validate_header(
                policy,
                "feat(core): update module and docs",
                ["src/core/main.rs", "docs/guide.md"],
            )

    def test_breaking_commit_allows_supporting_out_of_scope_paths(self) -> None:
        policy = make_policy()

        MODULE.validate_header(
            policy,
            "feat(core)!: rename public API and docs",
            ["src/core/main.rs", "docs/guide.md"],
        )

    def test_breaking_commit_requires_at_least_one_in_scope_path(self) -> None:
        policy = make_policy()

        with self.assertRaises(MODULE.ValidationError):
            MODULE.validate_header(
                policy,
                "feat(core)!: rewrite docs only",
                ["docs/guide.md"],
            )

    def test_breaking_commit_still_requires_narrowest_scope(self) -> None:
        policy = make_policy()

        with self.assertRaises(MODULE.ValidationError):
            MODULE.validate_header(
                policy,
                "feat(product)!: rename public API and docs",
                ["src/core/main.rs", "docs/guide.md"],
            )

    def test_header_only_validation_allows_valid_scope_without_changed_paths(self) -> None:
        policy = make_policy()

        MODULE.validate_header(policy, "chore(product): prepare squash merge title", [])


if __name__ == "__main__":
    unittest.main()
