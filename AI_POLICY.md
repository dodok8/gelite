# AI usage policy

This policy applies to contributions to Gelite. It is adapted for this project
after reviewing Fedify's AI usage policy:

https://github.com/fedify-dev/fedify/blob/main/AI_POLICY.md

AI tools are allowed in this repository, but AI output is not accepted as a
substitute for understanding, testing, or maintainership. Gelite is a learning
project that is also intended to become production-quality software, so the
person submitting a change remains responsible for the design, implementation,
tests, and documentation.

## Rules

- Disclose AI assistance in pull request descriptions and commit messages.
- Use an `Assisted-by` trailer for commits that include AI-assisted work.
- Do not use `Co-authored-by` for AI tools. That trailer is reserved for human
  contributors.
- AI-assisted pull requests must be manually reviewed by the contributor before
  submission.
- AI-assisted code must be tested in an environment the contributor can access.
  Do not submit code for platforms, tools, or database backends that were not
  manually checked.
- AI-generated explanations, issues, discussions, and documentation must be
  edited by a human before submission. Remove generic filler and keep only
  claims that match the code, specs, or tests.
- AI-generated diagrams or images are allowed only in documentation and must be
  clearly labeled with the tool used to create them.
- Do not submit AI-generated changes that bypass the repository specs. If a
  change conflicts with `spec/`, update the spec first or explain the mismatch
  in the pull request.
- Do not use AI to produce large rewrites without a narrow reviewable scope.
  Prefer small commits that preserve crate boundaries and include focused tests.

## Commit trailer format

When AI tools assisted a commit, add one trailer per tool:

```text
Assisted-by: AGENT_NAME:MODEL_VERSION
```

Use the actual tool and model identifiers for the commit. For example, work
assisted by Codex running `gpt-5.6-sol` uses:

```text
Assisted-by: Codex:gpt-5.6-sol
```

Example commit message:

```text
Document select pipeline boundaries

Add crate-level documentation for the schema, resolver, IR, and SQLite
planning stages.

Assisted-by: Codex:gpt-5.6-sol
```

## Human responsibility

Every submitted change must have a human owner. The owner is responsible for
checking that:

- the change matches the relevant `spec/` documents
- the implementation follows the crate responsibility boundaries
- tests cover the behavior being changed
- documentation describes the actual code state rather than a future design
- generated text does not contain unsupported claims

AI can help draft, inspect, and refactor. It cannot take responsibility for the
result.

## Maintainer discretion

Maintainers may reject or ask for revision of AI-assisted contributions that
are too broad, untested, noisy, misleading, or inconsistent with the project
documents. Repeated nondisclosure or low-quality AI-assisted submissions may
lead to contribution restrictions.
