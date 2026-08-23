use crate::tests::fixtures::{
    event_with_required_datetime_catalog, insert_scalar_types_catalog, post_only_catalog,
    post_with_author_lookup_catalog, post_with_only_required_author_catalog,
    post_with_optional_author_catalog, profile_with_optional_fields_catalog, select_assignment,
    user_only_catalog, user_with_only_multi_posts_catalog, user_with_optional_nickname_catalog,
    user_with_required_name_and_email_catalog, user_with_required_name_catalog,
    user_with_required_uuid_catalog,
};
use crate::{ResolveError, resolve_insert};
use alloc::string::ToString;
use alloc::vec;
use query_ast::{
    Assignment as AstAssignment, AssignmentOperator, AssignmentValue, CompareExpr, CompareOp, Expr,
    InsertQuery, Literal, Path, PathStep,
};

fn assignment(field_name: impl Into<alloc::string::String>, literal: Literal) -> AstAssignment {
    AstAssignment::new(field_name, AssignmentValue::Literal(literal))
}

fn equality_filter(field_name: &str, literal: Literal) -> Expr {
    Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new(field_name)])),
        CompareOp::Eq,
        Expr::Literal(literal),
    ))
}

fn assert_link_select_semantic_error(result: Result<query_ir::InsertQuery, ResolveError>) {
    let error = result.expect_err("link select assignment should not resolve");
    assert!(
        !matches!(
            error,
            ResolveError::UnsupportedExpr { ref expr_type } if expr_type == "select assignment"
        ),
        "link select should receive a semantic diagnostic"
    );
}

#[test]
fn resolves_insert_link_select_by_unique_scalar() {
    let catalog = post_with_author_lookup_catalog();
    let query = InsertQuery::new(
        "Post",
        vec![select_assignment(
            "author",
            "User",
            &["id"],
            Some(equality_filter(
                "email",
                Literal::String("sheri@example.com".to_string()),
            )),
            None,
        )],
    );

    let resolved = resolve_insert(&catalog, &query).expect("link select assignment should resolve");
    assert_eq!(resolved.assignments()[0].field().name(), "author");
}

#[test]
fn rejects_link_select_assigned_to_scalar_field() {
    let catalog = post_with_author_lookup_catalog();
    let query = InsertQuery::new(
        "Post",
        vec![select_assignment(
            "title",
            "User",
            &["id"],
            Some(equality_filter(
                "email",
                Literal::String("sheri@example.com".to_string()),
            )),
            None,
        )],
    );

    assert_link_select_semantic_error(resolve_insert(&catalog, &query));
}

#[test]
fn rejects_link_select_assigned_to_multi_link() {
    let catalog = user_with_only_multi_posts_catalog();
    let query = InsertQuery::new(
        "User",
        vec![select_assignment(
            "posts",
            "Post",
            &["id"],
            Some(equality_filter("id", Literal::String("post-1".to_string()))),
            None,
        )],
    );

    assert_link_select_semantic_error(resolve_insert(&catalog, &query));
}

#[test]
fn rejects_link_select_with_mismatched_root_type() {
    let catalog = post_with_author_lookup_catalog();
    let query = InsertQuery::new(
        "Post",
        vec![select_assignment(
            "author",
            "Post",
            &["id"],
            Some(equality_filter("id", Literal::String("post-1".to_string()))),
            None,
        )],
    );

    assert_link_select_semantic_error(resolve_insert(&catalog, &query));
}

#[test]
fn rejects_link_select_without_exact_id_projection() {
    let catalog = post_with_author_lookup_catalog();

    for projected_fields in [vec!["name"], vec!["id", "name"]] {
        let query = InsertQuery::new(
            "Post",
            vec![select_assignment(
                "author",
                "User",
                &projected_fields,
                Some(equality_filter(
                    "email",
                    Literal::String("sheri@example.com".to_string()),
                )),
                None,
            )],
        );

        assert_link_select_semantic_error(resolve_insert(&catalog, &query));
    }
}

#[test]
fn rejects_link_select_without_uniqueness_proof() {
    let catalog = post_with_author_lookup_catalog();
    let cases = [
        (None, None),
        (
            Some(equality_filter(
                "name",
                Literal::String("Sheri".to_string()),
            )),
            None,
        ),
        (None, Some(1)),
    ];

    for (filter, limit) in cases {
        let query = InsertQuery::new(
            "Post",
            vec![select_assignment("author", "User", &["id"], filter, limit)],
        );

        assert_link_select_semantic_error(resolve_insert(&catalog, &query));
    }
}

#[test]
fn rejects_link_select_with_invalid_lookup_literal() {
    let catalog = post_with_author_lookup_catalog();

    for literal in [Literal::Int64(42), Literal::Null] {
        let query = InsertQuery::new(
            "Post",
            vec![select_assignment(
                "author",
                "User",
                &["id"],
                Some(equality_filter("email", literal)),
                None,
            )],
        );

        assert_link_select_semantic_error(resolve_insert(&catalog, &query));
    }
}

#[test]
fn resolves_insert_root_object_type() {
    let catalog = post_only_catalog();

    let query = InsertQuery::new("Post", vec![]);

    let resolved = resolve_insert(&catalog, &query).expect("insert query resolves");
    assert_eq!(resolved.root_object_type().name(), "Post");

    let assignments = resolved.assignments();
    assert_eq!(assignments.len(), 0);
}

#[test]
fn resolves_insert_scalar_assignment() {
    let catalog = user_with_required_name_catalog();

    let query = InsertQuery::new(
        "User",
        vec![assignment("name", Literal::String("Sheri".to_string()))],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query resolves");
    assert_eq!(resolved.root_object_type().name(), "User");

    let assignments = resolved.assignments();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "name");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String("Sheri".to_string()))
    );
}

#[test]
fn resolves_insert_scalar_definition_order() {
    let catalog = user_with_required_name_and_email_catalog();

    let query = InsertQuery::new(
        "User",
        vec![
            assignment("name", Literal::String("Sheri".to_string())),
            assignment("email", Literal::String("sheri@tachibana.com".to_string())),
        ],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query resolves");
    assert_eq!(resolved.root_object_type().name(), "User");

    let assignments = resolved.assignments();
    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].field().name(), "name");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String("Sheri".to_string()))
    );
    assert_eq!(assignments[1].field().name(), "email");
    assert_eq!(
        assignments[1].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String(
            "sheri@tachibana.com".to_string()
        ))
    );
}

#[test]
fn resolves_insert_scalar_literal_types() {
    let catalog = insert_scalar_types_catalog();

    let query = InsertQuery::new(
        "User",
        vec![
            assignment("name", Literal::String("Sheri".to_string())),
            assignment("alive", Literal::Bool(true)),
            assignment("number", Literal::Int64(667)),
            assignment("weight", Literal::Float64(55.0)),
        ],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query resolves");
    assert_eq!(resolved.root_object_type().name(), "User");

    let assignments = resolved.assignments();
    assert_eq!(assignments.len(), 4);
    assert_eq!(assignments[0].field().name(), "name");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String("Sheri".to_string()))
    );
    assert_eq!(assignments[1].field().name(), "alive");
    assert_eq!(
        assignments[1].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::Bool(true))
    );

    assert_eq!(assignments[2].field().name(), "number");
    assert_eq!(
        assignments[2].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::Int64(667))
    );

    assert_eq!(assignments[3].field().name(), "weight");
    assert_eq!(
        assignments[3].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::Float64(55.0))
    );
}

#[test]
fn resolves_insert_uuid_string_literal() {
    let catalog = user_with_required_uuid_catalog();

    let query = InsertQuery::new(
        "User",
        vec![assignment(
            "external_id",
            Literal::String("00000000-0000-0000-0000-000000000001".to_string()),
        )],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");
    let assignments = resolved.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "external_id");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String(
            "00000000-0000-0000-0000-000000000001".to_string()
        ))
    );
}

#[test]
fn resolves_insert_datetime_string_literal() {
    let catalog = event_with_required_datetime_catalog();

    let query = InsertQuery::new(
        "Event",
        vec![assignment(
            "starts_at",
            Literal::String("2026-07-13T10:30:00Z".to_string()),
        )],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");
    let assignments = resolved.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "starts_at");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::Scalar(query_ir::Literal::String(
            "2026-07-13T10:30:00Z".to_string()
        ))
    );
}

#[test]
fn resolves_insert_single_link_string_as_link_id() {
    let catalog = post_with_only_required_author_catalog();

    let query = InsertQuery::new(
        "Post",
        vec![assignment(
            "author",
            Literal::String("00000000-0000-0000-0000-000000000001".to_string()),
        )],
    );

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");
    let assignments = resolved.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "author");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::LinkId("00000000-0000-0000-0000-000000000001".to_string())
    );
}

#[test]
fn resolves_insert_null_for_optional_scalar() {
    let catalog = user_with_optional_nickname_catalog();

    let query = InsertQuery::new("User", vec![assignment("nickname", Literal::Null)]);

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");
    let assignments = resolved.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "nickname");
    assert_eq!(
        assignments[0].value(),
        &query_ir::AssignmentValue::ScalarNull
    );
}

#[test]
fn resolves_insert_null_for_optional_single_link() {
    let catalog = post_with_optional_author_catalog();

    let query = InsertQuery::new("Post", vec![assignment("author", Literal::Null)]);

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");
    let assignments = resolved.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].field().name(), "author");
    assert_eq!(assignments[0].value(), &query_ir::AssignmentValue::LinkNull);
}

#[test]
fn resolves_insert_with_omitted_optional_fields() {
    let catalog = profile_with_optional_fields_catalog();

    let query = InsertQuery::new("Profile", vec![]);

    let resolved = resolve_insert(&catalog, &query).expect("insert query should resolve");

    assert_eq!(resolved.root_object_type().name(), "Profile");
    assert!(resolved.assignments().is_empty());
}

#[test]
fn rejects_insert_unknown_root_object_type() {
    let catalog = user_only_catalog();
    let query = InsertQuery::new("Missing", vec![]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::UnknownObjectType {
            name: "Missing".to_string()
        }
    );
}

#[test]
fn rejects_insert_unknown_assignment_field() {
    let catalog = user_only_catalog();
    let query = InsertQuery::new(
        "User",
        vec![assignment("nickname", Literal::String("Sheri".to_string()))],
    );

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::UnknownField {
            object_type: "User".to_string(),
            field: "nickname".to_string()
        }
    );
}

#[test]
fn rejects_insert_duplicate_assignment_field() {
    let catalog = user_with_required_name_catalog();
    let query = InsertQuery::new(
        "User",
        vec![
            assignment("name", Literal::String("Sheri".to_string())),
            assignment("name", Literal::String("Ellie".to_string())),
        ],
    );

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::DuplicateAssignment {
            object_type: "User".to_string(),
            field: "name".to_string(),
        }
    );
}

#[test]
fn rejects_insert_assignment_to_implicit_id() {
    let catalog = user_only_catalog();

    let query = InsertQuery::new(
        "User",
        vec![assignment(
            "id",
            Literal::String("00000000-0000-0000-0000-000000000001".to_string()),
        )],
    );

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::AssignmentToImplicitField {
            object_type: "User".to_string(),
            field: "id".to_string(),
        }
    )
}

#[test]
fn rejects_insert_incompatible_scalar_literal() {
    let catalog = user_with_required_name_catalog();
    let query = InsertQuery::new("User", vec![assignment("name", Literal::Int64(100))]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::IncompatibleAssignmentType {
            object_type: "User".to_string(),
            field: "name".to_string(),
            expected: "str".to_string(),
            actual: "int64".to_string(),
        }
    )
}

#[test]
fn rejects_insert_null_for_required_scalar() {
    let catalog = user_with_required_name_catalog();

    let query = InsertQuery::new("User", vec![assignment("name".to_string(), Literal::Null)]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::NullAssignmentToRequiredField {
            object_type: "User".to_string(),
            field: "name".to_string(),
        }
    )
}

#[test]
fn rejects_insert_null_for_required_single_link() {
    let catalog = post_with_only_required_author_catalog();

    let query = InsertQuery::new(
        "Post",
        vec![assignment("author".to_string(), Literal::Null)],
    );

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::NullAssignmentToRequiredField {
            object_type: "Post".to_string(),
            field: "author".to_string(),
        }
    )
}

#[test]
fn rejects_insert_missing_required_scalar() {
    let catalog = user_with_required_name_catalog();

    let query = InsertQuery::new("User", vec![]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::MissingRequiredField {
            object_type: "User".to_string(),
            field: "name".to_string(),
        }
    )
}

#[test]
fn rejects_insert_missing_required_single_link() {
    let catalog = post_with_only_required_author_catalog();

    let query = InsertQuery::new("Post", vec![]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::MissingRequiredField {
            object_type: "Post".to_string(),
            field: "author".to_string(),
        }
    )
}

#[test]
fn rejects_insert_non_string_single_link_value() {
    let catalog = post_with_only_required_author_catalog();

    let query = InsertQuery::new("Post", vec![assignment("author", Literal::Int64(42))]);

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::IncompatibleAssignmentType {
            object_type: "Post".to_string(),
            field: "author".to_string(),
            expected: "object id string".to_string(),
            actual: "int64".to_string(),
        }
    )
}

#[test]
fn rejects_insert_multi_link_assignment() {
    let catalog = user_with_only_multi_posts_catalog();

    let query = InsertQuery::new(
        "User",
        vec![assignment(
            "posts",
            Literal::String("00000000-0000-0000-0000-000000000001".to_string()),
        )],
    );

    let err = resolve_insert(&catalog, &query).expect_err("insert query should not resolve");

    assert_eq!(
        err,
        ResolveError::MultiLinkAssignmentUnsupported {
            object_type: "User".to_string(),
            field: "posts".to_string(),
        }
    )
}

#[test]
fn rejects_multi_link_operator_in_insert_ast() {
    let catalog = user_with_only_multi_posts_catalog();
    let select = select_assignment("posts", "Post", &["id"], None, None);
    let query = InsertQuery::new(
        "User",
        vec![AstAssignment::with_operator(
            "posts",
            AssignmentOperator::Add,
            select.value().clone(),
        )],
    );

    let error = resolve_insert(&catalog, &query).expect_err("insert add should not resolve");
    assert_eq!(
        error,
        ResolveError::InvalidMultiLinkMutation {
            object_type: "User".to_string(),
            field: "posts".to_string(),
            reason: "multi-link operations are only valid in updates".to_string(),
        }
    );
}
