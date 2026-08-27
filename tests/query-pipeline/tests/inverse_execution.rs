use gelite_commands::{apply_schema, compile_query, execute_query};
use sqlite_runner::{SQLiteCellValue, SQLiteQueryResult, native::NativeSQLiteRunner};

const SCHEMA: &str = r#"
type Department {
  required unique name: str
  multi link employees: Employee inverse department
  multi link members: Employee inverse departments
  multi link stored_employees: Employee
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
        "select Department { name, employees: { name } } filter .employees.active = true order by .name asc limit 1 offset 1",
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

#[test]
fn inverse_follow_up_does_not_shadow_relation_table_with_selected_link_alias() {
    let mut db = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema(r#"
        type Department { required unique name: str target_id: str multi link employees: Employee inverse departments }
        type Employee { required name: str multi link departments: Department link employee__departments: Department }
    "#, &mut db).expect("schema");
    run(&mut db, "insert Department { name := \"A\" }");
    run(&mut db, "insert Department { name := \"B\" }");
    run(
        &mut db,
        "insert Employee { name := \"타치바나 셰리\", employee__departments := (select Department { id } filter .name = \"B\") }",
    );
    run(
        &mut db,
        "update Employee set { departments += (select Department { id } filter .name = \"A\") }",
    );
    let result = run(
        &mut db,
        "select Department { employees: { name, employee__departments: { name } } } filter .name = \"A\"",
    );
    let SQLiteCellValue::List(employees) = &result.rows()[0][0] else {
        panic!("employees")
    };
    let SQLiteCellValue::Object(employee) = &employees[0] else {
        panic!("employee")
    };
    assert_eq!(
        employee[1].1,
        SQLiteCellValue::Object(vec![("name".into(), SQLiteCellValue::Text("B".into()))])
    );
}

#[test]
fn inverse_self_links_and_nullable_suffixes_preserve_match_existence() {
    let mut db = setup();
    run(
        &mut db,
        "update Employee filter .name = \"Other\" set { manager := (select Employee { id } filter .name = \"타치바나 셰리\") }",
    );
    let result = run(
        &mut db,
        "select Employee { name, reports: { name, manager: { name } } } filter .reports.active = true",
    );
    assert_eq!(names(&result), ["타치바나 셰리"]);
    assert_eq!(child_count(&result.rows()[0][1]), 1);
    let result = run(
        &mut db,
        "select Department { name } filter .employees.manager.name = null order by .name asc",
    );
    assert_eq!(
        names(&result),
        ["A", "B"],
        "empty C is not a null path match"
    );
    let result = run(
        &mut db,
        "select Department { name } filter .employees.manager.name != null",
    );
    assert_eq!(names(&result), ["A"]);
}

#[test]
fn inverse_exists_aliases_and_bind_order_survive_nested_membership_scopes() {
    let mut db = setup();
    let catalog = db.load_schema_catalog().expect("catalog");
    let ast = query_parser::parse_select(
        "select Department { name } filter .employees.active = true and .name = \"A\"",
    )
    .expect("parse");
    let ir = query_resolver::resolve_select(&catalog, &ast).expect("resolve");
    let plan = sqlite_query_plan::plan_select(&ir);
    assert!(
        plan.joins().is_empty(),
        "existential joins stay inside their own scope"
    );
    let statement = sqlite_query_sqlgen::render_select(&plan);
    assert_eq!(
        statement.bind_values(),
        &[
            sqlite_query_sqlgen::SQLiteBindValue::Bool(true),
            sqlite_query_sqlgen::SQLiteBindValue::String("A".into())
        ]
    );
    assert!(statement.sql().contains("EXISTS (SELECT 1"));
    let result = run(
        &mut db,
        "select Department { name } filter .id in (select Department { id } filter .employees.name = \"Other\")",
    );
    assert_eq!(names(&result), ["A"]);
}

#[test]
fn inverse_reads_follow_forward_transaction_rollback() {
    let mut db = setup();
    db.begin_transaction().expect("begin");
    run(
        &mut db,
        "update Employee filter .name = \"Solo\" set { department := null }",
    );
    assert!(
        run(
            &mut db,
            "select Department { name } filter .employees.name = \"Solo\""
        )
        .rows()
        .is_empty()
    );
    db.rollback_transaction().expect("rollback");
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter .employees.name = \"Solo\""
        )),
        ["B"]
    );
}

#[test]
fn inverse_catalog_survives_database_reopen() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("gelite-inverse-{}-{suffix}.db", std::process::id()));
    {
        let mut db = NativeSQLiteRunner::open(path.to_str().expect("path")).expect("database");
        apply_schema(SCHEMA, &mut db).expect("schema");
        run(&mut db, "insert Department { name := \"A\" }");
        run(
            &mut db,
            "insert Employee { name := \"타치바나 셰리\", active := true, department := (select Department { id } filter .name = \"A\") }",
        );
    }
    {
        let mut db = NativeSQLiteRunner::open(path.to_str().expect("path")).expect("reopen");
        let result = run(
            &mut db,
            "select Department { name, employees: { name } } filter .employees.active = true",
        );
        assert_eq!(names(&result), ["A"]);
        assert_eq!(child_count(&result.rows()[0][1]), 1);
        assert!(
            !db.table_exists("department__employees")
                .expect("table lookup")
        );
    }
    std::fs::remove_file(path).expect("remove test database");
}

#[test]
fn organization_example_declares_inverse_and_runs_documented_queries() {
    let source = include_str!("../../../examples/organization.geli");
    let catalog = schema_parser::parse_schema(source).expect("example schema");
    let schema_model::Field::Link(link) = catalog
        .find_field("Department", "employees")
        .expect("employees")
    else {
        panic!("link")
    };
    assert_eq!(link.inverse_field_name(), Some("department"));
    let mut db = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema(source, &mut db).expect("example schema application");
    let docs = include_str!("../../../docs/src/organization.md");
    let mut executed = 0;
    for block in docs.split("```text\n").skip(1) {
        let query = block.split("```").next().expect("code block").trim();
        if ["insert ", "select ", "update ", "delete "]
            .iter()
            .any(|prefix| query.starts_with(prefix))
        {
            run(&mut db, query);
            executed += 1;
        }
    }
    assert!(executed >= 10, "documented workflow must execute");
    let result = run(
        &mut db,
        "select Department { code, employees: { employee_no } } order by .code asc",
    );
    assert_eq!(
        child_count(&result.rows()[0][1]),
        0,
        "archive is empty after forward reassignment"
    );
    assert_eq!(
        child_count(&result.rows()[1][1]),
        3,
        "investigation sees the reassigned employee"
    );
}

fn setup_scoped() -> NativeSQLiteRunner {
    let mut db = setup();
    for name in ["A", "B"] {
        run(
            &mut db,
            &format!(
                "update Employee filter .department.name = \"{name}\" set {{ departments += (select Department {{ id }} filter .name = \"{name}\") }}"
            ),
        );
        run(
            &mut db,
            &format!(
                "update Department filter .name = \"{name}\" set {{ stored_employees += (select Employee {{ id }} filter .department.name = \"{name}\") }}"
            ),
        );
    }
    db
}

#[test]
fn scoped_predicates_bind_conjunction_to_one_child_for_all_relation_storage_kinds() {
    let mut db = setup_scoped();
    for relation in ["employees", "members", "stored_employees"] {
        for (filter, expected) in [
            (
                format!(".{relation}.name = \"타치바나 셰리\" and .{relation}.active = true"),
                vec!["A"],
            ),
            (
                format!("exists .{relation} {{ .name = \"타치바나 셰리\" and .active = true }}"),
                vec![],
            ),
            (
                format!("exists .{relation} {{ .name = \"Other\" and .active = true }}"),
                vec!["A"],
            ),
            (
                format!("exists .{relation} {{ .name = \"타치바나 셰리\" or .active = true }}"),
                vec!["A", "B"],
            ),
            (
                format!("exists .{relation} {{ .name != \"missing\" }}"),
                vec!["A", "B"],
            ),
            (
                format!("not exists .{relation} {{ .active = true }}"),
                vec!["C"],
            ),
            (
                format!("exists .{relation} {{ not .active = true }}"),
                vec!["A"],
            ),
            (
                format!("exists .{relation} {{ .nickname = null }}"),
                vec!["A", "B"],
            ),
            (
                format!("exists .{relation} {{ null != .nickname }}"),
                vec![],
            ),
            (
                format!("exists .{relation} {{ not .nickname = \"A\" }}"),
                vec![],
            ),
            (
                format!("not exists .{relation} {{ .nickname = \"A\" }}"),
                vec!["A", "B", "C"],
            ),
            (
                format!("exists .{relation} {{ .nickname = null or .nickname = \"A\" }}"),
                vec!["A", "B"],
            ),
            (
                format!(
                    "exists .{relation} {{ concat(.name, \"!\") in [\"Other!\"] and .active = true }}"
                ),
                vec!["A"],
            ),
        ] {
            let result = run(
                &mut db,
                &format!("select Department {{ name }} filter {filter} order by .name asc"),
            );
            assert_eq!(names(&result), expected, "{filter}");
        }
        let result = run(
            &mut db,
            &format!(
                "select Department {{ name, {relation}: {{ name }} }} filter exists .{relation} {{ .active = true }} order by .name asc limit 1"
            ),
        );
        assert_eq!(names(&result), ["A"]);
        assert_eq!(
            child_count(&result.rows()[0][1]),
            2,
            "returned children are unfiltered"
        );
        let result = run(
            &mut db,
            &format!(
                "select Department {{ name }} filter exists .{relation} {{ .name != \"missing\" }} order by .name asc limit 1 offset 1"
            ),
        );
        assert_eq!(
            names(&result),
            ["B"],
            "multiple child matches do not distort pagination"
        );
    }
    run(
        &mut db,
        "update Employee filter .name = \"타치바나 셰리\" set { active := true, nickname := \"A\" }",
    );
    for relation in ["employees", "members", "stored_employees"] {
        assert_eq!(
            names(&run(
                &mut db,
                &format!(
                    "select Department {{ name }} filter exists .{relation} {{ .name = \"타치바나 셰리\" and .active = true and .nickname != null }}"
                )
            )),
            ["A"]
        );
    }
}

#[test]
fn scoped_predicates_preserve_single_link_paths_aliases_and_bind_order() {
    let mut db = setup_scoped();
    run(
        &mut db,
        "update Department filter .name = \"C\" set { parent := (select Department { id } filter .name = \"A\") }",
    );
    run(
        &mut db,
        "update Employee filter .name = \"Other\" set { manager := (select Employee { id } filter .name = \"타치바나 셰리\") }",
    );
    for (filter, expected) in [
        (
            "exists .parent.employees { .name = \"Other\" and .active = true }",
            vec!["C"],
        ),
        (
            "not exists .parent.employees { .name = \"Other\" }",
            vec!["A", "B"],
        ),
        (
            "exists .employees { .manager.name = \"타치바나 셰리\" and .manager.active = false and .name = \"Other\" }",
            vec!["A"],
        ),
        (
            "exists .employees { .manager.name = null and .active = true }",
            vec!["B"],
        ),
        (
            "exists .employees { .manager.name = null or .manager.active = false }",
            vec!["A", "B"],
        ),
        ("exists .employees { .department.name = .name }", vec![]),
        (
            ".id in (select Department { id } filter exists .employees { .name = \"Other\" })",
            vec!["A"],
        ),
        (
            "exists .employees { .name = \"Other\" } and exists .employees { .name = \"타치바나 셰리\" }",
            vec!["A"],
        ),
    ] {
        assert_eq!(
            names(&run(
                &mut db,
                &format!("select Department {{ name }} filter {filter} order by .name asc")
            )),
            expected,
            "{filter}"
        );
    }
    let catalog = db.load_schema_catalog().expect("catalog");
    let ast = query_parser::parse_select("select Department { name } filter exists .employees { .manager.name = \"타치바나 셰리\" and .manager.active = false } and .name = \"A\"").expect("parse");
    let ir = query_resolver::resolve_select(&catalog, &ast).expect("resolve");
    let plan = sqlite_query_plan::plan_select(&ir);
    assert!(
        plan.joins().is_empty(),
        "scoped joins never enter the parent plan"
    );
    let statement = sqlite_query_sqlgen::render_select(&plan);
    assert_eq!(
        statement.bind_values(),
        &[
            sqlite_query_sqlgen::SQLiteBindValue::String("타치바나 셰리".into()),
            sqlite_query_sqlgen::SQLiteBindValue::Bool(false),
            sqlite_query_sqlgen::SQLiteBindValue::String("A".into()),
        ]
    );
    assert_eq!(statement.sql().matches("EXISTS (SELECT 1").count(), 1);
    assert_eq!(
        statement.sql().matches("LEFT JOIN").count(),
        1,
        "body joins are shared"
    );
}

#[test]
fn scoped_predicates_select_update_and_delete_the_same_parents() {
    for relation in ["employees", "members", "stored_employees"] {
        let mut db = setup_scoped();
        let filter = format!("exists .{relation} {{ .name = \"Other\" and .active = true }}");
        assert_eq!(
            names(&run(
                &mut db,
                &format!("select Department {{ name }} filter {filter}")
            )),
            ["A"]
        );
        run(
            &mut db,
            &format!("update Department filter {filter} set {{ name := \"Updated\" }}"),
        );
        assert_eq!(
            names(&run(
                &mut db,
                "select Department { name } order by .name asc"
            )),
            ["B", "C", "Updated"]
        );
        run(
            &mut db,
            &format!(
                "delete Department filter exists .{relation} {{ .name = \"타치바나 셰리\" and .active = true }}"
            ),
        );
        if relation == "employees" {
            // Inverse FK matches necessarily have incoming references. Deletion
            // still obeys the stored single link's RESTRICT policy.
            let catalog = db.load_schema_catalog().expect("catalog");
            let query = compile_query(&catalog, &format!("delete Department filter {filter}"))
                .expect("compile delete");
            let error =
                execute_query(&mut db, query).expect_err("referenced parent cannot be deleted");
            assert!(error.message().contains("FOREIGN KEY"));
            assert_eq!(
                names(&run(
                    &mut db,
                    &format!("select Department {{ name }} filter {filter}")
                )),
                ["Updated"]
            );
            continue;
        }
        run(&mut db, "update Employee set { department := null }");
        run(&mut db, &format!("delete Department filter {filter}"));
        assert_eq!(
            names(&run(
                &mut db,
                "select Department { name } order by .name asc"
            )),
            ["B", "C"]
        );
    }
}

#[test]
fn scoped_predicates_reject_unsupported_inputs_during_resolution() {
    let db = setup();
    let catalog = db.load_schema_catalog().expect("catalog");
    for query in [
        "select Department { name } filter exists .missing { .name = \"A\" }",
        "select Department { name } filter exists .parent { .name = \"A\" }",
        "select Department { name } filter exists .employees { .active = 1 }",
        "select Department { name } filter exists .employees { .name = null }",
        "select Department { name } filter exists .employees { .nickname < null }",
        "select Department { name } filter exists .employees { .departments.name = \"A\" }",
        "select Department { name } filter exists .employees { exists .reports { .active = true } }",
        "select Department { name } filter exists .employees { .active }",
        "select Department { name } filter exists .employees { true }",
        "select Department { found := exists .employees { .active = true } }",
        "select Department { name } order by exists .employees { .active = true }",
        "select Department { name } filter (exists .employees { .active = true }) = true",
        "select Department { name } filter .name in [exists .employees { .active = true }]",
        "select Department { name } filter str(exists .employees { .active = true }) = \"true\"",
    ] {
        let ast = query_parser::parse_select(query).expect("valid syntax");
        assert!(
            query_resolver::resolve_select(&catalog, &ast).is_err(),
            "accepted {query}"
        );
    }
}

#[test]
fn scoped_predicates_filter_multi_link_mutation_sources_and_targets() {
    let mut db = setup_scoped();
    run(
        &mut db,
        "update Department filter exists .employees { .name = \"Other\" and .active = true } set { stored_employees -= (select Employee { id } filter .active = false) }",
    );
    let result = run(
        &mut db,
        "select Department { stored_employees: { name } } filter .name = \"A\"",
    );
    assert_eq!(
        result.rows()[0][0],
        SQLiteCellValue::List(vec![SQLiteCellValue::Object(vec![(
            "name".into(),
            SQLiteCellValue::Text("Other".into())
        )])])
    );
    run(
        &mut db,
        "update Department filter .name = \"C\" set { stored_employees += (select Employee { id } filter exists .departments { .name = \"B\" }) }",
    );
    assert_eq!(
        names(&run(
            &mut db,
            "select Department { name } filter exists .stored_employees { .name = \"Solo\" } order by .name asc"
        )),
        ["B", "C"]
    );
}

#[test]
fn scoped_predicates_isolate_self_link_aliases_from_selected_links_and_subqueries() {
    let mut db = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema(
        r#"
        type Node {
          required unique name: str
          multi link __gelite_scope_root: Node
          multi link root: Node inverse __gelite_scope_root
          link __gelite_join_0: Node
        }
    "#,
        &mut db,
    )
    .expect("schema");
    for name in ["A", "B", "C"] {
        run(&mut db, &format!("insert Node {{ name := \"{name}\" }}"));
    }
    run(
        &mut db,
        "update Node filter .name = \"A\" set { __gelite_scope_root += (select Node { id } filter .name in [\"B\", \"C\"]) }",
    );
    run(
        &mut db,
        "update Node filter .name = \"B\" set { __gelite_join_0 := (select Node { id } filter .name = \"A\") }",
    );
    for (filter, expected) in [
        (
            "exists .__gelite_scope_root { .name = \"B\" and .__gelite_join_0.name = \"A\" and .__gelite_join_0.name != \"B\" }",
            vec!["A"],
        ),
        (
            "exists .__gelite_scope_root { .name = \"C\" and .__gelite_join_0.name = \"A\" }",
            vec![],
        ),
        ("exists .root { .name = \"A\" }", vec!["B", "C"]),
        (
            ".id in (select Node { id } filter exists .root { .name = \"A\" })",
            vec!["B", "C"],
        ),
    ] {
        assert_eq!(
            names(&run(
                &mut db,
                &format!(
                    "select Node {{ name, __gelite_join_0: {{ name }} }} filter {filter} order by .name asc"
                )
            )),
            expected,
            "{filter}"
        );
    }
}
