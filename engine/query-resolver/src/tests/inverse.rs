use crate::{ResolveError, resolve_insert, resolve_select, resolve_update};
use alloc::{boxed::Box, vec};
use query_ast::{
    Assignment, AssignmentOperator, AssignmentValue, InsertQuery, Path, PathStep, SelectQuery,
    Shape, ShapeItem, UpdateQuery,
};
use schema_model::{Cardinality, Field, LinkField, ObjectType, SchemaCatalog};

fn catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "Department",
            vec![Field::Link(LinkField::with_inverse(
                "employees",
                "Employee",
                Cardinality::Many,
                "department",
            ))],
        ),
        ObjectType::new(
            "Employee",
            vec![Field::Link(LinkField::new(
                "department",
                "Department",
                Cardinality::Optional,
            ))],
        ),
    ])
    .expect("inverse catalog")
}

fn employee_select() -> SelectQuery {
    SelectQuery::new(
        "Employee",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    )
}

#[test]
fn inverse_shape_resolves_forward_storage_reference() {
    let catalog = catalog();
    let shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("employees")]),
        Some(employee_select().shape().clone()),
    )]);
    let query = SelectQuery::new("Department", shape, None, vec![], None, None);
    let resolved = resolve_select(&catalog, &query).expect("inverse select");
    let query_ir::ResolvedShapeItem::Field(field) = &resolved.shape().items()[0] else {
        panic!("link shape")
    };
    let traversal = field.link_traversal().expect("resolved link traversal");
    assert_eq!(
        traversal.stored_field(),
        &catalog
            .find_field_ref("Employee", "department")
            .expect("forward ref")
    );
    assert_eq!(traversal.direction(), query_ir::LinkDirection::Inverse);
    assert_eq!(
        field.field(),
        &catalog
            .find_field_ref("Department", "employees")
            .expect("inverse ref")
    );
    assert_eq!(field.cardinality(), Cardinality::Many);
}

#[test]
fn inverse_assignments_are_readonly_for_insert_and_all_update_operators() {
    let catalog = catalog();
    for op in [
        AssignmentOperator::Assign,
        AssignmentOperator::Add,
        AssignmentOperator::Remove,
    ] {
        let assignment = Assignment::with_operator(
            "employees",
            op,
            AssignmentValue::Select(Box::new(employee_select())),
        );
        let query = UpdateQuery::new("Department", None, vec![assignment]);
        assert!(matches!(
            resolve_update(&catalog, &query),
            Err(ResolveError::AssignmentToInverseField { .. })
        ));
    }
    let query = InsertQuery::new(
        "Department",
        vec![Assignment::new(
            "employees",
            AssignmentValue::Literal(query_ast::Literal::Null),
        )],
    );
    assert!(matches!(
        resolve_insert(&catalog, &query),
        Err(ResolveError::AssignmentToInverseField { .. })
    ));
}

#[test]
fn multi_path_comparisons_resolve_as_independent_existence_scopes() {
    use super::fixtures::{filter_eq_string, filter_ne_null};
    let catalog = catalog();
    let filter = query_ast::Expr::And(
        Box::new(filter_eq_string(&["employees", "id"], "employee-id")),
        Box::new(query_ast::Expr::Not(Box::new(filter_ne_null(&[
            "employees",
            "department",
            "id",
        ])))),
    );
    let query = SelectQuery::new(
        "Department",
        Shape::new(vec![]),
        Some(filter),
        vec![],
        None,
        None,
    );
    let resolved = resolve_select(&catalog, &query).expect("multi comparisons");
    let query_ir::Expr::And(left, right) = resolved.filter().expect("filter") else {
        panic!("and")
    };
    assert!(matches!(left.as_ref(), query_ir::Expr::Exists(_)));
    let query_ir::Expr::Not(inner) = right.as_ref() else {
        panic!("outer negation")
    };
    let query_ir::Expr::Exists(exists) = inner.as_ref() else {
        panic!("existence scope")
    };
    assert_eq!(exists.path().result_cardinality(), Cardinality::Many);
    assert_eq!(
        exists.path().steps()[0]
            .link_traversal()
            .expect("link")
            .direction(),
        query_ir::LinkDirection::Inverse
    );
}

fn scoped_query(path: &[&str], predicate: query_ast::Expr) -> SelectQuery {
    SelectQuery::new(
        "Department",
        Shape::new(vec![]),
        Some(query_ast::Expr::Scoped(Box::new(
            query_ast::ScopedPredicate::new(
                Path::new(path.iter().copied().map(PathStep::new).collect()),
                predicate,
            ),
        ))),
        vec![],
        None,
        None,
    )
}

#[test]
fn scoped_predicate_resolves_body_paths_from_one_child() {
    use super::fixtures::{filter_eq_string, filter_ne_null};
    let catalog = catalog();
    let query = scoped_query(
        &["employees"],
        query_ast::Expr::And(
            Box::new(filter_eq_string(&["id"], "employee-id")),
            Box::new(filter_ne_null(&["department", "id"])),
        ),
    );
    let resolved = resolve_select(&catalog, &query).expect("scoped predicate");
    let query_ir::Expr::Scoped(scoped) = resolved.filter().expect("filter") else {
        panic!("scope")
    };
    assert_eq!(scoped.path().root_object_type().name(), "Department");
    assert_eq!(scoped.path().result_cardinality(), Cardinality::Many);
    let query_ir::Expr::And(left, right) = scoped.predicate() else {
        panic!("body and")
    };
    let query_ir::Expr::Compare(compare) = left.as_ref() else {
        panic!("child comparison")
    };
    let query_ir::ValueExpr::Path(path) = compare.left() else {
        panic!("child path")
    };
    assert_eq!(path.root_object_type().name(), "Employee");
    assert_eq!(path.result_cardinality(), Cardinality::Required);
    assert!(matches!(right.as_ref(), query_ir::Expr::IsNotNull(_)));
}

#[test]
fn scoped_predicate_rejects_invalid_targets_types_and_nested_relations() {
    use super::fixtures::{filter_eq_bool, filter_eq_string, filter_in_select};
    use alloc::string::ToString;
    let catalog = catalog();
    for (query, expected) in [
        (
            scoped_query(&["id"], filter_eq_string(&["id"], "x")),
            ResolveError::UnsupportedPath,
        ),
        (
            scoped_query(&["employees", "department"], filter_eq_string(&["id"], "x")),
            ResolveError::UnsupportedPath,
        ),
        (
            scoped_query(
                &["employees", "department", "employees"],
                filter_eq_string(&["id"], "x"),
            ),
            ResolveError::UnsupportedPath,
        ),
        (
            scoped_query(&["employees"], filter_eq_string(&["employees", "id"], "x")),
            ResolveError::UnknownField {
                object_type: "Employee".to_string(),
                field: "employees".to_string(),
            },
        ),
        (
            scoped_query(&["employees"], filter_eq_bool(&["id"], true)),
            ResolveError::IncompatibleOperandTypes {
                expected: "uuid".to_string(),
                actual: "bool".to_string(),
            },
        ),
        (
            scoped_query(
                &["employees"],
                filter_eq_string(&["department", "employees", "id"], "x"),
            ),
            ResolveError::UnsupportedPath,
        ),
        (
            scoped_query(
                &["employees"],
                scoped_query(&["department", "employees"], filter_eq_string(&["id"], "x"))
                    .filter()
                    .expect("nested filter")
                    .clone(),
            ),
            ResolveError::UnsupportedExpr {
                expr_type: "nested scoped predicate".to_string(),
            },
        ),
        (
            scoped_query(
                &["employees"],
                filter_in_select(&["id"], "Employee", &["id"], None),
            ),
            ResolveError::UnsupportedExpr {
                expr_type: "subquery in scoped predicate".to_string(),
            },
        ),
    ] {
        assert_eq!(resolve_select(&catalog, &query), Err(expected));
    }
}
