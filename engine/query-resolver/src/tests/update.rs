use alloc::string::ToString;
use alloc::vec;

use query_ast::{
    Assignment, AssignmentOperator, AssignmentValue, CompareExpr, CompareOp, Expr, Literal, Path,
    PathStep, UpdateQuery,
};

use crate::tests::fixtures::{
    post_with_author_catalog, post_with_author_lookup_catalog, post_with_title_catalog,
    select_assignment, user_only_catalog, user_with_only_multi_posts_catalog,
    user_with_posts_catalog, user_with_required_name_catalog,
};
use crate::{ResolveError, resolve_update};

fn assignment(field_name: impl Into<alloc::string::String>, literal: Literal) -> Assignment {
    Assignment::new(field_name, AssignmentValue::Literal(literal))
}

fn equality_filter(field: &str, literal: Literal) -> Expr {
    Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new(field)])),
        CompareOp::Eq,
        Expr::Literal(literal),
    ))
}

fn multi_link_assignment(
    operator: AssignmentOperator,
    root_type_name: &str,
    projected_fields: &[&str],
) -> Assignment {
    let select = select_assignment("posts", root_type_name, projected_fields, None, None);
    Assignment::with_operator("posts", operator, select.value().clone())
}

#[test]
fn resolves_multi_link_add_and_remove_without_at_most_one_proof() {
    let catalog = user_with_posts_catalog();

    for (operator, expected) in [
        (AssignmentOperator::Add, query_ir::AssignmentOperator::Add),
        (
            AssignmentOperator::Remove,
            query_ir::AssignmentOperator::Remove,
        ),
    ] {
        let query = UpdateQuery::new(
            "User",
            None,
            vec![multi_link_assignment(operator, "Post", &["id"])],
        );
        let resolved = resolve_update(&catalog, &query)
            .expect("multi-link mutation should accept a many-row target select");

        assert_eq!(resolved.assignments()[0].operator(), expected);
        assert!(matches!(
            resolved.assignments()[0].value(),
            query_ir::AssignmentValue::MultiLinkSelect(_)
        ));
    }
}

#[test]
fn rejects_multi_link_mutation_with_wrong_target_type() {
    let catalog = user_with_posts_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![multi_link_assignment(
            AssignmentOperator::Add,
            "User",
            &["id"],
        )],
    );

    let error = resolve_update(&catalog, &query).expect_err("wrong target type should fail");
    assert_eq!(
        error,
        ResolveError::InvalidMultiLinkMutation {
            object_type: "User".to_string(),
            field: "posts".to_string(),
            reason: "select root does not match the link target".to_string(),
        }
    );
}

#[test]
fn rejects_multi_link_mutation_on_single_link() {
    let catalog = post_with_author_catalog();
    let select = select_assignment("author", "User", &["id"], None, None);
    let query = UpdateQuery::new(
        "Post",
        None,
        vec![Assignment::with_operator(
            "author",
            AssignmentOperator::Add,
            select.value().clone(),
        )],
    );

    let error = resolve_update(&catalog, &query).expect_err("single link add should fail");
    assert_eq!(
        error,
        ResolveError::InvalidMultiLinkMutation {
            object_type: "Post".to_string(),
            field: "author".to_string(),
            reason: "assignment target is not a multi link".to_string(),
        }
    );
}

#[test]
fn rejects_multi_link_mutation_that_does_not_project_only_id() {
    let catalog = user_with_posts_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![multi_link_assignment(
            AssignmentOperator::Remove,
            "Post",
            &["view_count"],
        )],
    );

    let error = resolve_update(&catalog, &query).expect_err("non-id target shape should fail");
    assert_eq!(
        error,
        ResolveError::InvalidMultiLinkMutation {
            object_type: "User".to_string(),
            field: "posts".to_string(),
            reason: "select must project exactly the implicit id".to_string(),
        }
    );
}

#[test]
fn rejects_multi_link_mutation_mixed_with_regular_assignment() {
    let catalog = user_with_posts_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![
            multi_link_assignment(AssignmentOperator::Add, "Post", &["id"]),
            assignment("name", Literal::String("Sheri".to_string())),
        ],
    );

    let error = resolve_update(&catalog, &query).expect_err("mixed mutation should fail");
    assert_eq!(
        error,
        ResolveError::MultiLinkMutationMustBeExclusive {
            object_type: "User".to_string(),
        }
    );
}

#[test]
fn resolves_update_link_select_by_implicit_id() {
    let catalog = post_with_author_lookup_catalog();
    let query = UpdateQuery::new(
        "Post",
        None,
        vec![select_assignment(
            "author",
            "User",
            &["id"],
            Some(equality_filter("id", Literal::String("user-1".to_string()))),
            None,
        )],
    );

    let resolved = resolve_update(&catalog, &query).expect("link select assignment should resolve");
    assert_eq!(resolved.assignments()[0].field().name(), "author");
}

#[test]
fn resolves_update_target_filter_and_scalar_assignment() {
    let catalog = post_with_author_catalog();
    let query = UpdateQuery::new(
        "Post",
        Some(equality_filter("id", Literal::String("post-1".to_string()))),
        vec![assignment(
            "title",
            Literal::String("Closed Case".to_string()),
        )],
    );

    let resolved = resolve_update(&catalog, &query).expect("update query should resolve");

    assert_eq!(resolved.target_object_type().name(), "Post");
    assert!(resolved.filter().is_some());
    assert_eq!(resolved.assignments().len(), 1);
    assert_eq!(resolved.assignments()[0].field().name(), "title");
    assert_eq!(
        resolved.assignments()[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String("Closed Case".to_string()))
    );
}

#[test]
fn resolves_update_single_link_assignment_without_other_required_fields() {
    let catalog = post_with_author_catalog();
    let query = UpdateQuery::new(
        "Post",
        None,
        vec![assignment("author", Literal::String("user-2".to_string()))],
    );

    let resolved = resolve_update(&catalog, &query).expect("update query should resolve");

    assert_eq!(resolved.assignments().len(), 1);
    assert_eq!(resolved.assignments()[0].field().name(), "author");
    assert_eq!(
        resolved.assignments()[0].value(),
        &query_ir::AssignmentValue::LinkId("user-2".to_string())
    );
}

#[test]
fn rejects_update_empty_set() {
    let catalog = post_with_title_catalog();
    let query = UpdateQuery::new("Post", None, vec![]);

    let error = resolve_update(&catalog, &query).expect_err("empty update should not resolve");

    assert_eq!(
        error,
        ResolveError::EmptyUpdateSet {
            object_type: "Post".to_string(),
        }
    );
}

#[test]
fn rejects_update_unknown_target_type() {
    let catalog = user_only_catalog();
    let query = UpdateQuery::new(
        "Missing",
        None,
        vec![assignment("name", Literal::String("Sheri".to_string()))],
    );

    let error =
        resolve_update(&catalog, &query).expect_err("unknown update target should not resolve");

    assert_eq!(
        error,
        ResolveError::UnknownObjectType {
            name: "Missing".to_string(),
        }
    );
}

#[test]
fn rejects_update_unknown_filter_field() {
    let catalog = post_with_title_catalog();
    let query = UpdateQuery::new(
        "Post",
        Some(equality_filter(
            "missing",
            Literal::String("value".to_string()),
        )),
        vec![assignment("title", Literal::String("Closed".to_string()))],
    );

    let error =
        resolve_update(&catalog, &query).expect_err("unknown filter field should not resolve");

    assert_eq!(
        error,
        ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        }
    );
}

#[test]
fn rejects_update_unknown_assignment_field() {
    let catalog = post_with_title_catalog();
    let query = UpdateQuery::new(
        "Post",
        None,
        vec![assignment("missing", Literal::String("value".to_string()))],
    );

    let error =
        resolve_update(&catalog, &query).expect_err("unknown assignment field should not resolve");

    assert_eq!(
        error,
        ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        }
    );
}

#[test]
fn rejects_update_duplicate_assignment() {
    let catalog = user_with_required_name_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![
            assignment("name", Literal::String("Sheri".to_string())),
            assignment("name", Literal::String("Ellie".to_string())),
        ],
    );

    let error =
        resolve_update(&catalog, &query).expect_err("duplicate assignment should not resolve");

    assert_eq!(
        error,
        ResolveError::DuplicateAssignment {
            object_type: "User".to_string(),
            field: "name".to_string(),
        }
    );
}

#[test]
fn rejects_update_assignment_to_implicit_id() {
    let catalog = user_only_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![assignment("id", Literal::String("user-1".to_string()))],
    );

    let error = resolve_update(&catalog, &query).expect_err("id assignment should not resolve");

    assert_eq!(
        error,
        ResolveError::AssignmentToImplicitField {
            object_type: "User".to_string(),
            field: "id".to_string(),
        }
    );
}

#[test]
fn rejects_update_incompatible_scalar_literal() {
    let catalog = user_with_required_name_catalog();
    let query = UpdateQuery::new("User", None, vec![assignment("name", Literal::Int64(42))]);

    let error =
        resolve_update(&catalog, &query).expect_err("incompatible assignment should not resolve");

    assert_eq!(
        error,
        ResolveError::IncompatibleAssignmentType {
            object_type: "User".to_string(),
            field: "name".to_string(),
            expected: "str".to_string(),
            actual: "int64".to_string(),
        }
    );
}

#[test]
fn rejects_update_null_for_required_field() {
    let catalog = user_with_required_name_catalog();
    let query = UpdateQuery::new("User", None, vec![assignment("name", Literal::Null)]);

    let error =
        resolve_update(&catalog, &query).expect_err("required null assignment should not resolve");

    assert_eq!(
        error,
        ResolveError::NullAssignmentToRequiredField {
            object_type: "User".to_string(),
            field: "name".to_string(),
        }
    );
}

#[test]
fn rejects_update_multi_link_assignment() {
    let catalog = user_with_only_multi_posts_catalog();
    let query = UpdateQuery::new(
        "User",
        None,
        vec![assignment("posts", Literal::String("post-1".to_string()))],
    );

    let error =
        resolve_update(&catalog, &query).expect_err("multi-link assignment should not resolve");

    assert_eq!(
        error,
        ResolveError::MultiLinkAssignmentUnsupported {
            object_type: "User".to_string(),
            field: "posts".to_string(),
        }
    );
}
