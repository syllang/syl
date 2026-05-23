# Contributing

Contributions to Syl are welcome through GitHub issues and pull requests.

## Issues

Use issues to report bugs, request features, or discuss design questions.

When reporting a bug, include:

- a clear description of the observed behavior
- the behavior you expected
- the smallest reproduction you can provide
- relevant environment details
- any diagnostics, logs, or generated output that help explain the problem

For design discussions, describe the problem, the constraints, and the tradeoffs
behind the proposed direction.

## Pull Requests

Keep pull requests focused. A pull request should address one coherent change
and avoid unrelated cleanup.

Before opening a pull request:

- explain what changed and why
- describe how the change was validated
- call out user-visible behavior changes
- call out public API or compatibility impact
- keep generated files out of the diff unless they are intentionally tracked

Prefer small, reviewable commits with clear commit messages. If a change needs
multiple steps, make each step independently understandable.

## Review

Review focuses on correctness, maintainability, diagnostics, and long-term
architecture. Be prepared to justify design choices and simplify changes when a
smaller solution is enough.

Comments should be technical, specific, and respectful. Assume good intent, but
do not hide concrete concerns.

## Project Standards

- Preserve clear responsibility boundaries.
- Prefer explicit public exports.
- Avoid broad compatibility re-exports.
- Keep diagnostics structured.
- Keep source files reasonably small and easy to review.
- Avoid unresolved debt markers in committed code.

## License

By contributing, you agree that your contribution is licensed under the Apache
License, Version 2.0.
