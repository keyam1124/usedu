# Repository Guidelines

## Project Structure & Module Organization

This repository contains a Rust macOS disk usage CLI/TUI named `usedu`.
Public design constraints live in `docs/design.md` and `docs/design.ja.md`.

Keep Rust code under `src/`.
Scanner logic should stay shared by report and TUI modes.
Put integration tests in `tests/`; keep fixtures or sample output in `fixtures/` or `examples/`.

## Build, Test, and Development Commands

Use these commands once `Cargo.toml` exists:

- `cargo build`: compile the project.
- `cargo run -- report [PATH]`: run static report mode, e.g. `cargo run -- report ~/Library --depth 2`.
- `cargo run -- [PATH]`: run TUI mode.
- `cargo test --workspace`: run tests.
- `cargo fmt`: format Rust code.
- `cargo clippy --workspace --all-targets --all-features`: lint all targets.

Do not add Swift, GUI, daemon, or cleanup/delete behavior.

## Coding Style & Naming Conventions

Use `rustfmt`. Use `snake_case` for functions, variables, modules, and file names; use `PascalCase` for structs, enums, and traits. Prefer `anyhow` for app-level errors and `thiserror` for reusable domain errors.

Scanner code should use `symlink_metadata` and allocated-size semantics. Report the displayed size label as `Used`.

## Testing Guidelines

Test scanner behavior: hidden files, symlinks not followed, permission errors, cross-filesystem skipping, hard-link counting, and directory aggregation. Name integration tests by behavior, such as `tests/symlink_handling.rs`.

Use small temporary directory fixtures rather than scanning broad real paths such as `/`.

## Execution Rules

When a final LLM-based evaluation is required, delegate it to a read-only SubAgent with reasoning effort `low`. Do not replace it with the main agent or a high-reasoning agent.

## Git Operations

Branch names use `type/<task>-<slug>`, such as `feat/123-add-task-run-api` or `docs/git-operation-rules`. Align `type` with Conventional Commits: `feat`, `fix`, `docs`, `refactor`, `test`, or `chore`. Include `<task>` only for an agentflow Task, issue, or task number; otherwise omit it. Use lowercase English letters, numbers, and hyphens for `<slug>`.

Use Conventional Commits for commit messages: `<type>(<scope>): <summary>`. Keep branch type and commit type aligned.

Push or create PRs only after human confirmation. When AI creates a PR, check [.github/pull_request_template.md](.github/pull_request_template.md), follow its headings and checklist, and write `N/A` or a short reason for non-applicable items. PRs should include behavior changes, validation commands, and terminal screenshots or sample output for report/TUI rendering changes.

## Security & Configuration Tips

Never log secrets, tokens, or private file contents. Keep scanning read-only: do not delete, move, mutate, or recommend cleanup actions. Scanning `/` must require the user to pass `/` explicitly.
