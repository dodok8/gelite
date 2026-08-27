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
