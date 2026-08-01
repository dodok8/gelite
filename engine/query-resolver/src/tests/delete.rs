use alloc::string::ToString;
use alloc::vec;

use query_ast::{CompareExpr, CompareOp, DeleteQuery, Expr, Literal, Path, PathStep};

use crate::tests::fixtures::{post_with_title_catalog, user_only_catalog, user_with_posts_catalog};
use crate::{ResolveError, resolve_delete};

fn equality_filter(field: &str, literal: Literal) -> Expr {
    Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new(field)])),
        CompareOp::Eq,
        Expr::Literal(literal),
    ))
}

#[test]
fn resolves_delete_target_and_filter() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new(
        "Post",
        Some(equality_filter(
            "title",
            Literal::String("Draft".to_string()),
        )),
    );

    let resolved = resolve_delete(&catalog, &query).expect("delete query should resolve");

    assert_eq!(resolved.target_object_type().name(), "Post");
    assert!(resolved.filter().is_some());
}

#[test]
fn resolves_unfiltered_delete() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new("Post", None);

    let resolved = resolve_delete(&catalog, &query).expect("delete query should resolve");

    assert_eq!(resolved.target_object_type().name(), "Post");
    assert!(resolved.filter().is_none());
}

#[test]
fn rejects_delete_unknown_target_type() {
    let catalog = user_only_catalog();
    let query = DeleteQuery::new("Missing", None);

    let error = resolve_delete(&catalog, &query).expect_err("unknown target should fail");

    assert_eq!(
        error,
        ResolveError::UnknownObjectType {
            name: "Missing".to_string(),
        }
    );
}

#[test]
fn rejects_delete_unknown_filter_field() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new(
        "Post",
        Some(equality_filter(
            "missing",
            Literal::String("value".to_string()),
        )),
    );

    let error = resolve_delete(&catalog, &query).expect_err("unknown filter field should fail");

    assert_eq!(
        error,
        ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        }
    );
}

#[test]
fn rejects_delete_filter_through_multi_link() {
    let catalog = user_with_posts_catalog();
    let query = DeleteQuery::new(
        "User",
        Some(Expr::Compare(CompareExpr::new(
            Expr::Path(Path::new(vec![
                PathStep::new("posts"),
                PathStep::new("view_count"),
            ])),
            CompareOp::Eq,
            Expr::Literal(Literal::Int64(1)),
        ))),
    );

    assert_eq!(
        resolve_delete(&catalog, &query),
        Err(ResolveError::UnsupportedPath)
    );
}
