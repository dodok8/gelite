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
