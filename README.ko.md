# Gelite

Gelite는 Gel 같은 query language를 실용적으로 재현해 보는 Rust 프로젝트입니다.

목표는 Gel의 코드베이스를 복제하거나 모든 database feature를 한 번에 다시
만드는 것이 아닙니다. 목표는 Gel에서 유용한 언어 아이디어를 더 작은 Rust
codebase 안에서 직접 구현해 보는 것입니다.

- table-first modeling 대신 object type 중심 모델링
- object 사이의 explicit link
- shape를 가진 `select` query
- schema-aware name resolution
- typed intermediate representation
- ordinary SQLite SQL로 lowering

이 프로젝트는 학습 프로젝트이기도 합니다. 중요한 compiler 단계를 숨기지 않고
crate와 타입으로 드러내서, Gel 같은 query language가 어떤 과정을 거쳐 SQL로
내려가는지 직접 조사하고 테스트하고 확장할 수 있게 만드는 것이 목적입니다.

## 이 프로젝트가 증명하려는 것

Gel의 query language가 유용한 이유 중 하나는 query가 반환받을 object shape를
직접 말할 수 있다는 점입니다.

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
```

이 방식은 직접 join을 조립하고 application code에서 nested object를 다시 만드는
것보다 읽기 쉽습니다.

Gelite는 더 작은 질문에서 출발합니다.

Gel 같은 query language를 SQLite를 target으로 하는 작은 Rust engine으로
구현할 수 있는가?

현재는 이 질문에 답하기 위해 다음 pipeline을 한 단계씩 만들고 있습니다.

```text
query text
  -> syntax tree
  -> schema-resolved Semantic IR
  -> SQLite-specific plan
  -> SQL text + bind values
  -> SQLite execution
  -> logical result shape
```

## 현재 범위

Gelite의 현재 범위는 다음과 같습니다.

- query compilation: `select`, `insert`, `update`, `delete` parsing, semantic
  resolution, SQLite query planning, SQL rendering
- nested single-link result shaping을 포함한 현재 `select`, `insert`, `update`,
  `delete` subset의 native query execution
- 사전 compile과 explicit transaction validation을 적용하는 semicolon 기반
  multi-statement query file
- database-backed interactive REPL의 explicit `start transaction`, `commit`,
  `rollback` command
- initial schema planning: `.geli` parsing, SQLite schema planning, DDL SQL
  rendering

Initial schema를 SQLite database에 적용할 수 있고, 현재 query subset은 CLI
REPL을 통해 실행할 수 있습니다. 아직 migration diffing, server, web UI는
없습니다.

이건 현재 단계의 의도입니다. runtime feature를 올리기 전에 language pipeline과
schema pipeline이 정확하고 이해 가능한지 먼저 검증하는 것이 첫 번째 유효한
milestone입니다.

## 예시

schema model은 현재 Rust catalog value로 존재합니다. 모델링 중인 언어는 다음과
같습니다.

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

다음 query가 들어오면:

```text
select Post {
  title,
  author: {
    name
  }
}
filter .title = "Hello"
order by .title desc
limit 10
```

Gelite는 query를 parse하고, schema catalog에 맞춰 이름을 resolve하고, Semantic
IR을 만들고, SQLite plan을 만든 뒤, 대략 다음과 같은 SQL을 렌더링할 수 있습니다.

```sql
SELECT "root"."title", "author"."id", "author"."name"
FROM "post" AS "root"
INNER JOIN "user" AS "author" ON "root"."author_id" = "author"."id"
WHERE "root"."title" = ?
ORDER BY "root"."title" DESC
LIMIT 10
```

정확한 SQL 문자열 자체보다 중요한 것은 query meaning이 typed하고 inspectable한
단계를 거친 뒤 SQL로 나온다는 점입니다.

## 왜 단계를 나누는가

이 프로젝트는 text에서 SQL로 바로 컴파일하는 지름길을 피합니다.

각 단계는 하나의 책임을 가집니다.

- Parser: source text를 syntax로 바꿉니다.
- Schema catalog: object type, field, link, cardinality, implicit `id`를
  저장합니다.
- Resolver: catalog를 기준으로 이름과 shape rule을 검증합니다.
- Semantic IR: backend detail 없이 query의 resolved meaning을 기록합니다.
- SQLite planner: table, column, alias, join, predicate, result-shaping
  metadata를 결정합니다.
- SQL generator: SQLite plan을 SQL text와 bind value로 렌더링합니다.

이 구조는 Gel-like language semantics와 SQLite-specific storage decision을
분리합니다. 동시에 각 compiler step을 따로 조사할 수 있어서 학습에도 좋습니다.

Insert compilation은 implicit `id` bind value로 매번 새로운 UUID v4를
생성합니다. 따라서 렌더링된 insert bind output은 비결정적이며 stable
snapshot이나 재현 가능한 plan artifact로 사용하면 안 됩니다.

## 구현된 것

- `schema-model`: object type, scalar field, link, cardinality, deterministic
  reference, implicit `id` lookup을 가진 semantic schema catalog.
- `schema-parser`: 현재 `.geli` schema syntax용 lexer/parser.
- `query-ast`: data query와 transaction command용 unresolved syntax tree.
- `query-parser`: 현재 query syntax용 lexer/parser와 source span.
- `query-resolver`: select, insert, update, delete용 AST-to-IR semantic analysis.
- `query-ir`: 지원되는 query용 backend-independent Semantic IR.
- `sqlite-query-plan`: SQLite-specific structured query plan.
- `sqlite-query-sqlgen`: bind placeholder 기반 SQL renderer.
- `sqlite-schema-plan`: SQLite-specific initial schema plan.
- `sqlite-schema-sqlgen`: initial schema DDL과 metadata insert를 렌더링하는 SQL
  renderer.
- `sqlite-runner`: nested single-link result shaping을 포함한 native schema,
  query, transaction execution.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: 공유 query compilation 및 execution orchestration.
- `tools/repl`: 현재 pipeline을 query 하나로 확인하는 inspection tool.

## 아직 구현되지 않은 것

- Migration diffing과 migration history.
- 선택된 multi-link fetching과 result shaping.
- HTTP API.
- Web playground.

## 실행 방법

Organization 예제 schema를 local SQLite database에 적용합니다.

```sh
cargo run -p gelite-cli -- schema apply examples/organization.geli --database organization.db
```

Database-backed REPL을 엽니다.

```sh
cargo run -p gelite-cli -- repl --database organization.db
```

기존 Gelite database에서 query file을 실행합니다.

```sh
cargo run -p gelite-cli -- query run query.geliql --database organization.db
```

별도 문서에는 실행 가능한 예제 세 가지, REPL 입력 방법과 현재 출력 제약이
정리되어 있습니다.

- [시작하기](docs/src/README.md)
- [Examples](docs/src/examples.md)
- [Organization](docs/src/organization.md)
- [Store](docs/src/store.md)
- [Music catalog](docs/src/music.md)
- [CLI reference](docs/src/cli.md)
- [현재 제약](docs/src/limitations.md)

CI와 같은 mdBook version을 설치한 뒤 local site를 실행합니다.

```sh
cargo install mdbook --version 0.5.4 --locked
mdbook serve docs
```

mdBook 없이도 source Markdown을 직접 읽을 수 있습니다.

전체 project check는 `cargo test --workspace`로 실행합니다.

## 저장소 안내

`spec/`은 language와 engine stage의 의미를 정의합니다.

- `spec/schema.md`: schema language와 catalog semantics.
- `spec/query.md`: MVP query language surface.
- `spec/ir.md`: Semantic IR contract.
- `spec/storage-sqlite.md`: SQLite storage mapping.
- `spec/sqlite-query-plan.md`: SQLite query planning contract.

`plan/`은 구현 순서와 설계 근거를 설명합니다.

- `plan/new-db-engine-plan.md`
- `plan/new-db-engine-design.md`
- `plan/implementation-start-plan.md`
- `plan/query-parser-implementation-plan.md`
- `plan/select-path-traversal-plan.md`
- `plan/sqlite-query-plan-implementation-plan.md`
- `plan/sqlite-schema-plan-implementation-plan.md`
- `plan/cli-and-tooling-plan.md`

문서가 충돌하면 의미는 `spec/`, 작업 순서는 `plan/`을 우선합니다.

## 개발 원칙

Gelite는 Gel 같은 query compiler가 어떻게 동작하는지 배우기 위해 중요한 조각을
작은 시스템 안에서 다시 만드는 프로젝트입니다.

학습 목적이 있다고 해서 기준을 낮추지는 않습니다. 이 프로젝트는 production
foundation에 기대하는 기준을 유지해야 합니다.

- 계약이 분명한 작은 feature
- semantic behavior를 고정하는 test
- 명시적인 crate boundary
- direct AST-to-SQL shortcut 금지
- 현재 있는 것과 아직 없는 것을 정확히 말하는 documentation

다음 기술 목표는 선택된 multi link를 fetch하고 shape하는 것입니다.
