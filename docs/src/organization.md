# Organization

This `EMP`/`DEPT`-style example demonstrates a required department link and an
optional self-referencing manager link.

```text
{{#include ../../examples/organization.geli}}
```

## Create data

Apply the schema to a new database:

```sh
cargo run -p gelite-cli -- schema apply examples/organization.geli --database organization.db
```

Open `gelite repl --database organization.db`, then insert two departments:

```text
insert Department { code := "INVESTIGATION", name := "Investigation" }
```

```text
insert Department { code := "ARCHIVE", name := "Records Archive" }
```

Insert the manager first. Link assignments look up existing objects through
their unique fields, so generated IDs do not need to be copied from earlier
commands:

```text
insert Employee {
  employee_no := "MG-667",
  name := "Sheri Tachibana",
  title := "Chief Investigator",
  salary := 92000,
  active := true,
  hired_at := "2026-04-01T09:00:00Z",
  department := (
    select Department { id }
    filter .code = "INVESTIGATION"
  )
}
```

```text
insert Employee {
  employee_no := "MG-001",
  name := "Emma Sakuraba",
  title := "Investigator",
  salary := 68000,
  active := true,
  hired_at := "2026-04-15T09:00:00Z",
  department := (
    select Department { id }
    filter .code = "INVESTIGATION"
  ),
  manager := (
    select Employee { id }
    filter .employee_no = "MG-667"
  )
}
```

```text
insert Employee {
  employee_no := "MG-002",
  name := "Hiro Nikaido",
  title := "Archivist",
  salary := 64000,
  active := true,
  hired_at := "2026-05-01T09:00:00Z",
  department := (
    select Department { id }
    filter .code = "ARCHIVE"
  ),
  manager := null
}
```

## Query nested links

```text
select Employee {
  employee_no,
  name,
  title,
  department: {
    code,
    name
  },
  manager: {
    name,
    title
  }
}
filter .active = true
  and .department.id in (
    select Department { id }
    filter .code in ["INVESTIGATION", "ARCHIVE"]
  )
  and .employee_no not in ["MG-999"]
order by .salary desc, .name asc
limit 10
offset 0
```

The nested select finds matching department identities in its own query scope.
The employee-number condition keeps a literal-list membership example beside
it.

Find top-level employees whose optional manager link is absent:

```text
select Employee { employee_no, name, title }
filter .manager.id = null
order by .name asc
```

The REPL preserves the top-level field order and renders selected links as
nested objects. A missing optional link is `NULL`:

```text
employee_no\tname\ttitle\tdepartment\tmanager
MG-667\tSheri Tachibana\tChief Investigator\t{code: INVESTIGATION, name: Investigation}\tNULL
```
