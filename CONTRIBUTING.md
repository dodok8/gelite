# Contributing to Gelite

Gelite is a learning project that is also expected to grow into usable,
production-quality software. The code may be small, but crate boundaries, type
contracts, tests, and documentation should be chosen as if later work will
depend on them.

The project explores a Gel-like query language and schema model implemented in
Rust on top of SQLite. It is not a port of Gel's implementation. Reuse concepts
from Gel where they help, but do not copy its Postgres-specific or
Python/Cython-specific structure.

## Repository documents

Use these files as the source of contribution rules:

- [CONTRIBUTING.md](CONTRIBUTING.md): contribution workflow, validation, and
  code conventions.
- [AGENTS.md](AGENTS.md): additional instructions for AI agents working in this
  repository.
- [AI_POLICY.md](AI_POLICY.md): AI assistance disclosure, commit trailers, and
  human responsibility.
- [.github/pull_request_template.md](.github/pull_request_template.md):
  required pull request structure.
- [.github/ISSUE_TEMPLATE/](.github/ISSUE_TEMPLATE/): issue templates and
  required issue fields.
- [spec/](spec/): behavior, meaning, and layer contracts.
- [plan/](plan/): sequencing, scope control, and implementation rationale.

## Instruction priority

When instructions conflict, use this priority:

1. Explicit maintainer constraints for the current task, such as `review only`,
   `do not edit files`, `stop`, or a narrowed task scope.
2. Repository documents: [AGENTS.md](AGENTS.md),
   [CONTRIBUTING.md](CONTRIBUTING.md), [AI_POLICY.md](AI_POLICY.md), and
   `.github` templates.
3. The current issue's explicit instructions, including branch, scope, and
   acceptance criteria.
4. Relevant `spec/` documents.
5. Relevant `plan/` documents.
6. Other instructions for the current task.
7. Local implementation convenience.

If the current conversation is explicitly about changing repository guidance,
use it as the basis for the document update and make the resulting rule clear
in the changed document.

## Current repository state

The repository contains a Rust workspace with compiler and tooling crates under
`engine/`, `tools/`, and `tests/`.

Current implemented areas include:

- `engine/schema-model`: semantic schema catalog, object types, scalar fields,
  links, cardinality, deterministic references, and implicit `id` lookup.
- `engine/schema-parser`: lexer and parser for the current `.geli` schema
  syntax.
- `engine/query-ast`: unresolved syntax tree for select, insert, and update
  queries.
- `engine/query-parser`: lexer and parser for the current query syntax with
  source spans.
- `engine/query-resolver`: AST-to-IR semantic analysis for select, insert, and
  update queries.
- `engine/query-ir`: backend-independent Semantic IR for supported queries.
- `engine/sqlite-query-plan`: SQLite-specific structured query plans.
- `engine/sqlite-query-sqlgen`: SQL renderer for query plans.
- `engine/sqlite-schema-plan`: SQLite-specific initial schema plan.
- `engine/sqlite-schema-sqlgen`: SQL renderer for initial schema DDL and
  metadata inserts.
- `engine/sqlite-runner`: native SQLite schema and query execution.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: command orchestration shared by CLI-facing tools.
- `tools/repl`: query pipeline inspection and execution tool.
- `tests/query-pipeline`: cross-crate query pipeline tests.

The project can parse schema and query sources, plan and apply an initial
SQLite schema, and execute the current select, insert, and update subsets
through CLI/REPL workflows. It does not yet provide delete, migration diffing
and migration history, nested result shaping, an HTTP server, or a web UI.

## Issue workflow

Before changing files for an issue:

1. Read the full issue body.
2. Check the Branch field. If it is present, use that exact branch.
3. If no branch is specified, create or use
   `issue-<issue-number>-<short-description>`.
4. Check the scope, acceptance criteria, related specs, and related plans.
5. Check [.github/pull_request_template.md](.github/pull_request_template.md)
   so the eventual PR has the required information.
6. If the issue scope is unclear, write a plan before implementation.

Do not treat a request to handle an issue as permission to edit files when the
issue or conversation is still ambiguous. Clarify the intended scope first.

## Example workflows

Use these examples as starting points. If an issue gives different scope or
acceptance criteria, follow the issue and explain the difference in the pull
request.

### Documentation-only issue

1. Read the issue body and use the branch named in the issue.
2. Check whether the change affects `CONTRIBUTING.md`, `AGENTS.md`,
   `AI_POLICY.md`, `README.md`, `spec/`, or `plan/`.
3. Update the relevant document.
4. Run:

   ```sh
   git diff --check
   ```

5. If no code, examples, Cargo metadata, or documented command behavior changed,
   do not run `cargo test --workspace`. State the reason in the pull request.
6. Fill [.github/pull_request_template.md](.github/pull_request_template.md).

### Query syntax change

1. Read the issue, [spec/query.md](spec/query.md), and the relevant plan
   document.
2. Update [spec/query.md](spec/query.md) first if the syntax or meaning
   changes.
3. Update parser tests in `engine/query-parser`.
4. Update AST, resolver, IR, planner, or SQL generation only if the new syntax
   reaches those layers.
5. Add pipeline tests in `tests/query-pipeline` only when cross-crate behavior
   changes.
6. Run focused tests first, then:

   ```sh
   cargo test --workspace
   ```

### Resolver behavior change

1. Read the issue, [spec/query.md](spec/query.md), and [spec/ir.md](spec/ir.md).
2. Confirm the parser already represents the syntax needed for the change.
3. Update `engine/query-resolver`.
4. Add resolver tests for success and failure paths.
5. Update `engine/query-ir` only if the backend-independent meaning changes.
6. Run the relevant resolver tests, then `cargo test --workspace`.

### SQLite planning change

1. Read [spec/ir.md](spec/ir.md),
   [spec/storage-sqlite.md](spec/storage-sqlite.md), and
   [spec/sqlite-query-plan.md](spec/sqlite-query-plan.md).
2. Keep SQLite table, column, alias, and join decisions inside SQLite-specific
   crates.
3. Update `engine/sqlite-query-plan` or `engine/sqlite-query-sqlgen`.
4. Add tests at the planning or SQL rendering layer.
5. Add `tests/query-pipeline` coverage only if behavior across the full
   pipeline changes.

### AI-assisted change

1. Read [AI_POLICY.md](AI_POLICY.md).
2. Use AI output as a draft, not as authority.
3. Verify the change against the relevant spec, plan, tests, and code.
4. Disclose AI usage in the pull request template.
5. Add the required commit trailer:

   ```text
   Assisted-by: Codex:gpt-5.5
   ```

## Document changes

Documentation changes should describe the actual repository state. Do not use
documentation to advertise unsupported behavior.

For process or repository guidance changes, update the relevant document as
part of the first step of the work. For example:

- Update [CONTRIBUTING.md](CONTRIBUTING.md) when the contribution workflow,
  branch naming, test expectations, review expectations, or code conventions
  change.
- Update [AGENTS.md](AGENTS.md) when AI-agent-only behavior changes.
- Update [AI_POLICY.md](AI_POLICY.md) when AI assistance disclosure or
  responsibility rules change.
- Update [README.md](README.md) when user-facing features, examples, or
  commands change.

Documentation-only pull requests may skip `cargo test --workspace` when they do
not change code, examples that are compiled or executed, Cargo metadata, or
documented command behavior. In that case, run `git diff --check`, check links
and file references, and state why code tests were not needed.

## Spec and plan changes

The repository has `spec/` and `plan/` documents. They are not interchangeable.

- `spec/` defines behavior, meaning, and layer contracts.
- `plan/` defines sequencing, scope control, and implementation rationale.

Update `spec/` when a change affects syntax, query or schema meaning, Semantic
IR shape, SQLite storage rules, planner contracts, SQL generation contracts, or
runtime behavior contracts.

Update `plan/` when a change affects implementation order, milestone scope,
temporary strategy, or the intended sequence of work.

If implementation must diverge from a spec, report the divergence clearly.
Prefer updating the spec first when the intended meaning has changed.

## Relevant specs

Read the relevant spec before changing behavior in that area.

- [spec/schema.md](spec/schema.md): minimum schema language, field kinds,
  `link`, `required`, `multi`, scalar types, object types, and implicit `id`.
- [spec/query.md](spec/query.md): MVP query language syntax and supported query
  surface.
- [spec/ir.md](spec/ir.md): Semantic IR between AST/resolution and backend
  planning.
- [spec/storage-sqlite.md](spec/storage-sqlite.md): SQLite physical storage
  rules, tables, columns, links, and metadata.
- [spec/sqlite-query-plan.md](spec/sqlite-query-plan.md): SQLite-specific
  planning layer between Semantic IR and SQL generation.

Tests should usually assert spec contracts at the lowest layer that owns them.

## Relevant plans

Use plan documents to place work in the current build sequence and explain why
the scope is appropriate.

- [plan/new-db-engine-plan.md](plan/new-db-engine-plan.md): product scope, MVP
  boundaries, major components, and broad build sequence.
- [plan/new-db-engine-design.md](plan/new-db-engine-design.md): top-level
  architecture and component responsibilities.
- [plan/implementation-start-plan.md](plan/implementation-start-plan.md):
  original first slice for catalog, AST, IR, and resolver. Useful for intent,
  but the repository has now moved beyond parts of this plan.
- [plan/schema-parser-implementation-plan.md](plan/schema-parser-implementation-plan.md):
  schema parser sequencing and parser contracts.
- [plan/query-parser-implementation-plan.md](plan/query-parser-implementation-plan.md):
  query parser sequencing and parser contracts.
- [plan/query-expression-model-plan.md](plan/query-expression-model-plan.md):
  query expression model work.
- [plan/select-path-traversal-plan.md](plan/select-path-traversal-plan.md):
  select path traversal semantics.
- [plan/sqlite-schema-plan-implementation-plan.md](plan/sqlite-schema-plan-implementation-plan.md):
  SQLite schema planning implementation sequence.
- [plan/sqlite-query-plan-implementation-plan.md](plan/sqlite-query-plan-implementation-plan.md):
  SQLite query planner implementation sequence.
- [plan/cli-and-tooling-plan.md](plan/cli-and-tooling-plan.md): CLI, REPL, and
  tooling workflows.

## Pipeline boundaries

Preserve the staged pipeline unless there is a deliberate spec or plan change:

```text
schema/query source
  -> lexer/parser
  -> AST or schema model
  -> resolver / semantic validation
  -> backend-independent IR
  -> SQLite-specific plan
  -> SQL text + bind values
  -> runner/runtime behavior
```

Keep syntax concerns in parser/AST crates, semantic concerns in model/resolver
and IR crates, SQLite decisions in SQLite planning/sqlgen/runner crates, and
CLI orchestration in `tools/`.

Examples of boundary violations to avoid:

- A parser deciding SQLite table or column names.
- SQL generation re-resolving source-level field names.
- `query-ir` depending on SQLite aliasing or join strategy.
- CLI code duplicating parser, planner, or runner behavior instead of
  delegating to shared command or engine crates.

## Code conventions

Prefer the patterns already present in the repository.

- Keep public struct fields private unless there is a clear reason not to.
- Use `new` for simple constructors that cannot fail.
- Use `try_new` and a typed error when construction can reject invalid state.
- Prefer typed values such as `ObjectTypeRef`, `FieldRef`, `Cardinality`, and
  `ScalarType` over stringly typed glue.
- Preserve deterministic ordering where tests or public APIs rely on it.
- Avoid convenience APIs that make invalid schema, query, or plan states easy
  to construct.
- Keep each crate responsible for its own layer.

Most `engine/*` crates are `#![no_std]`. Do not add `std`-only requirements to
those crates without a deliberate design change.

## Errors and panics

Library and semantic layers should report recoverable failures with typed
errors. Existing examples include `SchemaError`, `ResolveError`,
`ResolvedPathError`, `ParseError`, and `LexError`.

- Do not panic for invalid user input.
- Avoid `unwrap` in production code.
- Use `expect` only for internal invariants that were already checked nearby,
  and include a message explaining the invariant.
- Tests and fixtures may use `expect` and `panic!` when the message explains
  the failed contract.
- Do not leave `todo!` or `unimplemented!` on mainline code paths. Represent
  intentional unsupported behavior with an explicit error variant.

## Tests and fixtures

Put tests at the layer that owns the contract:

- Parser syntax belongs in parser crate tests.
- Schema invariants belong in `schema-model` or `schema-parser` tests.
- Resolver semantics belong in `query-resolver` tests.
- IR construction invariants belong in `query-ir` tests.
- SQLite physical planning belongs in `sqlite-query-plan` tests.
- SQL rendering belongs in `sqlite-query-sqlgen` or `sqlite-schema-sqlgen`
  tests.
- Cross-crate behavior belongs in `tests/query-pipeline`.

Use `src/tests/fixtures.rs` when setup is shared by multiple tests. A repeated
setup used by two or more tests is a fixture candidate, but do not hide the
assertion being tested behind a fixture helper.

Raw SQL fixtures are acceptable only when the Gelite pipeline for that behavior
does not exist yet. Add a short comment that explains the temporary rule and
what should replace it later.

## Dependencies

Add dependencies conservatively.

- Do not add a dependency when the standard library or an existing dependency
  is sufficient.
- Keep backend and tooling dependencies out of syntax, semantic, and IR crates.
- Add CLI-only dependencies under `tools/*`, not engine crates.
- Distinguish runtime dependencies from dev-dependencies.
- Check `no_std` compatibility before adding a dependency to an engine crate
  that currently uses `#![no_std]`.
- In the pull request, explain why the dependency is needed and which crate owns
  the need.

## Rust version

The workspace uses Rust edition 2024 through `[workspace.package]` in
`Cargo.toml`. The repository does not currently declare an MSRV with
`rust-version`.

Until an MSRV is declared, validate changes with the current stable Rust
toolchain available to the contributor.

## Documentation style

Repository documentation should be written in clear, concrete English. It
should record the actual code and design state, not sell the project.

Prefer:

- Actual crate, type, and function names.
- Small claims that can be checked against code or tests.
- Explicit unsupported cases.
- Short notes about temporary rules and what will replace them.
- One technical purpose per paragraph.

Avoid:

- Generic claims such as "scalable", "robust", "powerful", or "flexible"
  unless the mechanism is named.
- Inflated phrasing such as "plays a crucial role", "marks a significant
  step", or "underscores the importance".
- Unsupported generalizations such as "best practices suggest".
- Template-like summaries at the end of every section.
- Decorative formatting, unnecessary tables, and forced three-item lists.

Comments in code should explain boundaries, invariants, or non-obvious reasons.
Do not comment behavior that is already clear from the code.

Every crate should have crate-level `//!` docs that state its responsibility and
non-goals. Public types and functions should be documented when they define a
layer contract, invariant, error condition, or non-obvious behavior. Do not add
noise documentation to simple getters.

## Reviewing changes

Prioritize defects, behavioral regressions, and missing tests over summaries or
style preferences. Review repository-specific risks first:

- mismatches with the relevant `spec/` documents
- crate boundary leaks, especially backend-specific decisions outside SQLite
  crates
- incorrect type, cardinality, resolver, or planner behavior
- happy-path-only handling that omits supported failure cases
- unclear ownership hidden by plausible names or abstractions
- tests that assert output shape without checking the semantic contract
- abstractions introduced before there are at least two concrete users

Report findings in severity order with file and line references. If no defect
is found, say so and identify remaining test gaps or assumptions.

## Validation

Run focused tests while developing. Run the workspace tests for changes that
affect shared contracts or cross-crate pipeline behavior.

Pull request validation is enforced by the Rust workflow in
[.github/workflows/rust.yml](.github/workflows/rust.yml). Local hooks are a
convenience only; CI is the authoritative validation gate.

Default full validation:

```sh
cargo test --workspace
```

Useful command checks:

```sh
cargo run -p gelite-cli -- --help
cargo run -p gelite-cli -- schema plan examples/blog.geli
```

For formatting and lint checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Report any command that could not be run.

## Pre-commit hooks

This repository provides a `.pre-commit-config.yaml` for local convenience.
Pre-commit hooks do not replace pull request validation and can be bypassed
locally. The required checks are the Rust workflow checks.

Install and run hooks with:

```sh
pre-commit install
pre-commit run --all-files
```

The hooks check Rust formatting and Clippy warnings. If the hook environment is
not available, run the equivalent Cargo commands manually and report that the
pre-commit hook itself was not run.

## Pull requests

Use [.github/pull_request_template.md](.github/pull_request_template.md).

A pull request should:

- state the behavior or documentation contract it changes
- mention the issue it closes
- list the relevant `spec/` and `plan/` files checked
- describe the main implementation choices
- list the validation commands that were run
- identify unsupported behavior or follow-up work
- disclose AI assistance according to [AI_POLICY.md](AI_POLICY.md)

If a pull request changes behavior, include tests at the layer that owns the
contract. If a pull request only changes documentation or repository metadata,
explain why code tests were not needed.

## AI-assisted work

AI tools are allowed in this repository, but AI output is not a substitute for
understanding, testing, or maintainership. Follow [AI_POLICY.md](AI_POLICY.md).

Commits that include Codex-assisted design, implementation, review, or
documentation must include this trailer:

```text
Assisted-by: Codex:gpt-5.5
```

Use professional commit messages. Do not use conversational or character voice
in commits, code, comments, docs, JSON, YAML, SQL, or other artifacts.
