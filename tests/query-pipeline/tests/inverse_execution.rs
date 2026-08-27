use gelite_commands::{apply_schema, compile_query, execute_query};
use sqlite_runner::{SQLiteCellValue, SQLiteQueryResult, native::NativeSQLiteRunner};

const SCHEMA: &str = r#"
type Department {
  required unique name: str
  multi link employees: Employee inverse department
  multi link members: Employee inverse departments
  link parent: Department
}
type Employee {
  required unique name: str
  required active: bool
  nickname: str
  link department: Department
  multi link departments: Department
  link manager: Employee
  multi link reports: Employee inverse manager
}
"#;

fn run(db: &mut NativeSQLiteRunner, source: &str) -> SQLiteQueryResult {
    let catalog = db.load_schema_catalog().expect("reload persisted catalog");
    let query = compile_query(&catalog, source).expect(source);
    execute_query(db, query).expect(source)
}

fn setup() -> NativeSQLiteRunner {
    let mut db = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema(SCHEMA, &mut db).expect("schema");
    for name in ["A", "B", "C"] {
        run(
            &mut db,
            &format!("insert Department {{ name := \"{name}\" }}"),
        );
    }
    for (name, department, active) in [
        ("타치바나 셰리", "A", false),
        ("Other", "A", true),
        ("Solo", "B", true),
    ] {
        run(
            &mut db,
            &format!(
                "insert Employee {{ name := \"{name}\", active := {active}, department := (select Department {{ id }} filter .name = \"{department}\") }}"
            ),
        );
    }
    db
}

fn child_count(value: &SQLiteCellValue) -> usize {
    let SQLiteCellValue::List(children) = value else {
        panic!("expected child list: {value:?}")
    };
    children.len()
}

#[test]
fn inverse_fk_shapes_zero_one_many_and_observes_forward_changes() {
    let mut db = setup();
    let result = run(
        &mut db,
        "select Department { name, employees: { name } } order by .name asc",
    );
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| child_count(&row[1]))
            .collect::<Vec<_>>(),
        [2, 1, 0]
    );
    run(
        &mut db,
        "update Employee filter .name = \"Solo\" set { department := (select Department { id } filter .name = \"C\") }",
    );
    let result = run(
        &mut db,
        "select Department { name, employees: { name } } order by .name asc",
    );
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| child_count(&row[1]))
            .collect::<Vec<_>>(),
        [2, 0, 1]
    );
    run(
        &mut db,
        "update Employee filter .name = \"Solo\" set { department := null }",
    );
    run(&mut db, "delete Employee filter .name = \"Other\"");
    let result = run(
        &mut db,
        "select Department { name, employees: { name } } order by .name asc",
    );
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| child_count(&row[1]))
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
}

#[test]
fn inverse_join_table_shapes_observe_add_remove_and_nested_shapes() {
    let mut db = setup();
    run(
        &mut db,
        "update Employee set { departments += (select Department { id } filter .name = \"A\") }",
    );
    let result = run(
        &mut db,
        "select Department { name, members: { name, department: { name, employees: { name } } } } order by .name asc",
    );
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| child_count(&row[1]))
            .collect::<Vec<_>>(),
        [3, 0, 0]
    );
    let SQLiteCellValue::List(members) = &result.rows()[0][1] else {
        panic!("members")
    };
    for member in members {
        let SQLiteCellValue::Object(fields) = member else {
            panic!("member")
        };
        let SQLiteCellValue::Object(department) = &fields[1].1 else {
            panic!("department")
        };
        let expected = if department[0].1 == SQLiteCellValue::Text("A".into()) {
            2
        } else {
            1
        };
        assert_eq!(child_count(&department[1].1), expected);
    }
    run(
        &mut db,
        "update Employee filter .name = \"Solo\" set { departments -= (select Department { id } filter .name = \"A\") }",
    );
    let result = run(
        &mut db,
        "select Department { members: { name } } filter .name = \"A\"",
    );
    assert_eq!(child_count(&result.rows()[0][0]), 2);
}

fn names(result: &SQLiteQueryResult) -> Vec<String> {
    result
        .rows()
        .iter()
        .map(|row| {
            let SQLiteCellValue::Text(name) = &row[0] else {
                panic!("name")
            };
            name.clone()
        })
        .collect()
}

#[test]
fn inverse_filters_preserve_existential_boolean_null_and_pagination_semantics() {
    let mut db = setup();
    for (filter, expected) in [
        (".employees.active = true", vec!["A", "B"]),
        ("true = .employees.active", vec!["A", "B"]),
        (
            ".employees.name = \"타치바나 셰리\" and .employees.active = true",
            vec!["A"],
        ),
        ("not (.employees.name = \"타치바나 셰리\")", vec!["B", "C"]),
        (".employees.name != \"타치바나 셰리\"", vec!["A", "B"]),
        (".employees.nickname = null", vec!["A", "B"]),
        (".employees.nickname != null", vec![]),
        ("not (.employees.nickname = null)", vec!["C"]),
        (".employees.name = \"missing\" or .name = \"C\"", vec!["C"]),
    ] {
        let result = run(
            &mut db,
            &format!("select Department {{ name }} filter {filter} order by .name asc"),
        );
        assert_eq!(names(&result), expected, "{filter}");
    }
    let result = run(
        &mut db,
        "select Department { name, employees: { name } } filter .employees.active = true order by .name asc offset 1 limit 1",
    );
    assert_eq!(names(&result), ["B"]);
    let result = run(
        &mut db,
        "select Department { name, employees: { name } } filter .employees.active = true order by .name asc limit 1",
    );
    assert_eq!(names(&result), ["A"]);
    assert_eq!(
        child_count(&result.rows()[0][1]),
        2,
        "parent filter does not filter children"
    );
}

#[test]
fn stored_and_inverse_multi_filters_support_chains_and_mutations() {
    let mut db = setup();
    run(
        &mut db,
        "update Employee set { departments += (select Department { id } filter .name = \"A\") }",
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter .members.active = true"
        )),
        ["A"]
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Employee { name } filter .departments.name = \"A\" order by .name asc"
        )),
        ["Other", "Solo", "타치바나 셰리"]
    );
    run(
        &mut db,
        "update Department filter .name = \"C\" set { parent := (select Department { id } filter .name = \"A\") }",
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter .parent.employees.active = true"
        )),
        ["C"]
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter .employees.departments.name = \"A\" order by .name asc"
        )),
        ["A", "B"]
    );
    run(
        &mut db,
        "update Department filter .members.name = \"Solo\" set { name := \"Updated\" }",
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter .members.name = \"Solo\""
        )),
        ["Updated"]
    );
    run(
        &mut db,
        "delete Employee filter .departments.name = \"Updated\"",
    );
    assert!(
        run(
            &mut db,
            "select Department { name } filter .members.active = true"
        )
        .rows()
        .is_empty()
    );
}

#[test]
fn unsupported_multi_value_expressions_and_inverse_writes_fail_before_sql() {
    let db = setup();
    let catalog = db.load_schema_catalog().expect("catalog");
    for query in [
        "select Department { names := .employees.name }",
        "select Department { name } order by .employees.name",
        "select Department { name } filter .employees.name = .name",
        "select Department { name } filter .employees.name in [\"Other\"]",
        "select Department { name } filter concat(.employees.name, \"\") = \"Other\"",
        "select Department { name } filter .employees.active = 1",
        "select Department { name } filter .employees.name = null",
        "select Department { name } filter .employees.nickname < null",
        "insert Department { name := \"D\", employees := null }",
        "update Department set { employees += (select Employee { id }) }",
        "update Department set { employees -= (select Employee { id }) }",
        "update Department set { employees := (select Employee { id } filter .name = \"Other\") }",
    ] {
        assert!(compile_query(&catalog, query).is_err(), "accepted {query}");
    }
}
