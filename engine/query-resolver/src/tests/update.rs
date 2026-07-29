use alloc::string::ToString;
use alloc::vec;

use query_ast::{Assignment, CompareExpr, CompareOp, Expr, Literal, Path, PathStep, UpdateQuery};

use crate::tests::fixtures::{
    post_with_author_catalog, post_with_title_catalog, user_only_catalog,
    user_with_only_multi_posts_catalog, user_with_required_name_catalog,
};
use crate::{ResolveError, resolve_update};

fn equality_filter(field: &str, literal: Literal) -> Expr {
    Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new(field)])),
        CompareOp::Eq,
        Expr::Literal(literal),
    ))
}

#[test]
fn resolves_update_target_filter_and_scalar_assignment() {
    let catalog = post_with_author_catalog();
    let query = UpdateQuery::new(
        "Post",
        Some(equality_filter("id", Literal::String("post-1".to_string()))),
        vec![Assignment::new(
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
        vec![Assignment::new(
            "author",
            Literal::String("user-2".to_string()),
        )],
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
        vec![Assignment::new(
            "name",
            Literal::String("Sheri".to_string()),
        )],
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
        vec![Assignment::new(
            "title",
            Literal::String("Closed".to_string()),
        )],
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
        vec![Assignment::new(
            "missing",
            Literal::String("value".to_string()),
        )],
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
            Assignment::new("name", Literal::String("Sheri".to_string())),
            Assignment::new("name", Literal::String("Ellie".to_string())),
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
        vec![Assignment::new("id", Literal::String("user-1".to_string()))],
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
    let query = UpdateQuery::new(
        "User",
        None,
        vec![Assignment::new("name", Literal::Int64(42))],
    );

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
    let query = UpdateQuery::new("User", None, vec![Assignment::new("name", Literal::Null)]);

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
        vec![Assignment::new(
            "posts",
            Literal::String("post-1".to_string()),
        )],
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
