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
```

## 현재 범위

Gelite의 현재 범위는 다음과 같습니다.

- query compilation: `select`, `insert`, `update` parsing, semantic
  resolution, SQLite query planning, SQL rendering
- 현재 `select`, `insert`, `update` subset의 native query execution
- initial schema planning: `.geli` parsing, SQLite schema planning, DDL SQL
  rendering

Initial schema를 SQLite database에 적용할 수 있고, 현재 query subset은 CLI
REPL을 통해 실행할 수 있습니다. 아직 migration diffing, `delete`, server,
web UI는 없습니다.

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

## 구현된 것

- `schema-model`: object type, scalar field, link, cardinality, deterministic
  reference, implicit `id` lookup을 가진 semantic schema catalog.
- `schema-parser`: 현재 `.geli` schema syntax용 lexer/parser.
- `query-ast`: select, insert, update query용 unresolved syntax tree.
- `query-parser`: 현재 query syntax용 lexer/parser와 source span.
- `query-resolver`: select, insert, update용 AST-to-IR semantic analysis.
- `query-ir`: 지원되는 query용 backend-independent Semantic IR.
- `sqlite-query-plan`: SQLite-specific structured query plan.
- `sqlite-query-sqlgen`: bind placeholder 기반 SQL renderer.
- `sqlite-schema-plan`: SQLite-specific initial schema plan.
- `sqlite-schema-sqlgen`: initial schema DDL과 metadata insert를 렌더링하는 SQL
  renderer.
- `sqlite-runner`: native schema와 query statement execution.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: CLI-facing tool들이 공유하는 command orchestration.
- `tools/repl`: 현재 pipeline을 query 하나로 확인하는 inspection tool.

## 아직 구현되지 않은 것

- `gelite query plan`, `gelite query run`.
- Delete.
- Migration diffing과 migration history.
- Runtime nested result shaping.
- HTTP API.
- Web playground.

## 실행 방법

전체 테스트:

```sh
cargo test --workspace
```

현재 CLI 실행:

```sh
cargo run -p gelite-cli -- --help
```

`gelite` binary를 직접 설치하거나 빌드한 경우에는 아래 예시에서
`cargo run -p gelite-cli --` prefix를 빼고 사용할 수 있습니다.

### 현재 사용 가능한 CLI 명령

현재 CLI에서 실제로 동작하는 명령 경로는 세 가지입니다.

```text
gelite schema plan <schema.geli>
gelite schema apply <schema.geli> --database <app.db>
gelite repl --schema <schema.geli> [--debug] [QUERY]...
gelite repl --database <app.db> [--debug] [QUERY]...
```

`gelite schema plan <schema.geli>`는 schema source file을 parse하고, initial
SQLite schema plan을 만들고, SQL과 metadata bind value를 출력합니다. 이 명령은
database를 열거나 변경하지 않습니다.

예시 schema file:

```text
type User {
  required email: str
}

type Post {
  required title: str
  required link author: User
}
```

schema planning 실행:

```sh
cargo run -p gelite-cli -- schema plan examples/blog.geli
```

schema를 SQLite database에 적용:

```sh
cargo run -p gelite-cli -- schema apply examples/blog.geli --database app.db
```

`gelite repl --schema <schema.geli>`는 schema source file에서 parse한 catalog를
기준으로 현재 query inspection pipeline을 실행합니다. query 인자가 없으면
interactive REPL을 시작하고, query 인자가 있으면 그 query 하나를 parse하고 SQL로
렌더링합니다.

`gelite repl --database <app.db>`는 SQLite database 안의 Gelite metadata table에서
catalog를 읽고, select, insert, update query를 실제 database에 실행합니다.
`--debug`가 없으면 result row, 생성된 insert ID 또는 update된 row 수를 출력하고,
`--debug`가 있으면 rendered SQL과 bind value를 먼저 출력합니다.

CLI REPL 실행:

```sh
cargo run -p gelite-cli -- repl --database app.db
```

`examples/blog.geli`를 적용한 뒤 interactive REPL에서 다음 query들을 실행할 수
있습니다.

User insert:

```text
insert User {
  email := "alice@example.com"
}
```

REPL이 생성한 UUID를 출력합니다. 이 값을 link된 Post를 insert할 때 사용합니다.

```text
insert Post {
  title := "Draft",
  view_count := 5,
  author := "<generated-user-id>"
}
```

Insert한 Post와 author 조회:

```text
select Post {
  id,
  title,
  view_count,
  author: {
    email
  }
}
```

Link된 object를 기준으로 Post update:

```text
update Post
filter .author.email = "alice@example.com"
set {
  title := "Reviewed"
}
```

Update 결과 확인:

```text
select Post {
  title,
  author: {
    email
  }
}
filter .title = "Reviewed"
```

query 하나 실행:

```sh
cargo run -p gelite-cli -- repl --database app.db 'select Post { title, author: { email } } filter .title = "Hello" order by .title desc limit 10'
```

Membership filter 실행:

```sh
cargo run -p gelite-cli -- repl --database app.db 'select Post { title, author: { email } } filter .title in ["Draft", "Published"] and .author.email not in ["blocked@example.com"] order by .title asc limit 20'
```

result row 전에 SQL과 bind value 출력:

```sh
cargo run -p gelite-cli -- repl --database app.db --debug 'select Post { title, author: { email } } filter .title = "Hello"'
```

CLI REPL은 숨겨진 default catalog를 사용하지 않습니다. `--schema`나 `--database`
가 없으면 사용 안내 성격의 error를 내고 종료합니다.

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

다음 기술 목표는 delete pipeline을 추가하고, SQLite 결과를 logical nested
object로 shape하는 것입니다.
