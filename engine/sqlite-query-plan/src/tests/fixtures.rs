use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use schema_model::{FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

pub fn post_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(1), "Post")
}

pub fn post_title_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), post_type(), "title")
}

pub fn post_author_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), post_type(), "author")
}

pub fn post_best_friend_field() -> FieldRef {
    FieldRef::new(FieldId::new(6), post_type(), "best_friend")
}

pub fn post_generated_join_name_field() -> FieldRef {
    FieldRef::new(FieldId::new(7), post_type(), "__gelite_join_0")
}

pub fn post_view_count_field() -> FieldRef {
    FieldRef::new(FieldId::new(4), post_type(), "view_count")
}

pub fn post_id_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), post_type(), "id")
}

pub fn empty_post_insert_query() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(post_type(), vec![])
}

pub fn post_insert_with_title_assignment() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![query_ir::Assignment::new(
            post_title_field(),
            query_ir::AssignmentValue::Scalar(query_ir::Literal::String("Case File".to_string())),
        )],
    )
}

pub fn post_insert_with_ordered_assignments() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![
            query_ir::Assignment::new(
                post_view_count_field(),
                query_ir::AssignmentValue::Scalar(query_ir::Literal::Int64(7)),
            ),
            query_ir::Assignment::new(
                post_title_field(),
                query_ir::AssignmentValue::Scalar(query_ir::Literal::String(
                    "Case File".to_string(),
                )),
            ),
            query_ir::Assignment::new(
                post_author_field(),
                query_ir::AssignmentValue::LinkId(
                    "00000000-0000-0000-0000-000000000001".to_string(),
                ),
            ),
        ],
    )
}

pub fn post_insert_with_author_select_assignment() -> query_ir::InsertQuery {
    let user_id = user_id_field();
    let select = query_ir::SelectQuery::new(
        user_type(),
        query_ir::ResolvedShape::new(
            user_type(),
            vec![query_ir::ResolvedShapeField::new(
                "id",
                user_id.clone(),
                schema_model::Cardinality::Required,
                None,
            )],
        ),
        Some(query_ir::Expr::Compare(query_ir::CompareExpr::new(
            query_ir::ValueExpr::Path(
                query_ir::ResolvedPath::try_new(
                    user_type(),
                    vec![query_ir::ResolvedPathStep::scalar(
                        user_id,
                        schema_model::Cardinality::Required,
                    )],
                )
                .expect("user id path should be valid"),
            ),
            query_ir::CompareOp::Eq,
            query_ir::ValueExpr::Literal(query_ir::Literal::String("user-1".to_string())),
        ))),
        vec![],
        None,
        None,
    );

    query_ir::InsertQuery::new(
        post_type(),
        vec![query_ir::Assignment::new(
            post_author_field(),
            query_ir::AssignmentValue::LinkSelect(alloc::boxed::Box::new(select)),
        )],
    )
}

pub fn post_title_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![query_ir::ResolvedPathStep::scalar(
                post_title_field(),
                schema_model::Cardinality::Required,
            )],
        )
        .expect("post title path should be valid"),
    )
}

pub fn post_id_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![query_ir::ResolvedPathStep::scalar(
                post_id_field(),
                schema_model::Cardinality::Required,
            )],
        )
        .expect("post id path should be valid"),
    )
}

pub fn post_view_count_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![query_ir::ResolvedPathStep::scalar(
                post_view_count_field(),
                schema_model::Cardinality::Required,
            )],
        )
        .expect("post view_count path should be valid"),
    )
}

pub fn post_author_name_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    post_author_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_name_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("post author name path should be valid"),
    )
}

pub fn post_author_score_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    post_author_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_score_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("post author score path should be valid"),
    )
}

pub fn post_best_friend_name_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    post_best_friend_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_name_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("post best_friend name path should be valid"),
    )
}

pub fn post_generated_join_name_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    post_generated_join_name_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_name_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("post generated join name path should be valid"),
    )
}

pub fn user_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(2), "User")
}

pub fn user_id_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), user_type(), "id")
}

pub fn user_name_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), user_type(), "name")
}

pub fn user_score_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), user_type(), "score")
}

pub fn user_best_friend_field() -> FieldRef {
    FieldRef::new(FieldId::new(4), user_type(), "best_friend")
}

pub fn user_posts_field() -> FieldRef {
    FieldRef::new(FieldId::new(5), user_type(), "posts")
}

pub fn user_best_friend_score_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            user_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    user_best_friend_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_score_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("user best_friend score path should be valid"),
    )
}

pub fn empty_post_query() -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        None,
    )
}

pub fn post_title_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema_model::Cardinality::Required,
        None,
    )
}

pub fn post_id_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "id",
        post_id_field(),
        schema_model::Cardinality::Required,
        None,
    )
}

pub fn user_name_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "name",
        user_name_field(),
        schema_model::Cardinality::Required,
        None,
    )
}

pub fn post_author_shape_field() -> query_ir::ResolvedShapeField {
    let author_shape = query_ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    )
}

pub fn user_best_friend_shape_field() -> query_ir::ResolvedShapeField {
    let best_friend_shape =
        query_ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "best_friend",
        user_best_friend_field(),
        schema_model::Cardinality::Required,
        Some(best_friend_shape),
    )
}

pub fn user_best_friend_with_best_friend_shape_field() -> query_ir::ResolvedShapeField {
    let best_friend_shape =
        query_ir::ResolvedShape::new(user_type(), vec![user_best_friend_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "best_friend",
        user_best_friend_field(),
        schema_model::Cardinality::Required,
        Some(best_friend_shape),
    )
}

pub fn post_author_with_best_friend_shape_field() -> query_ir::ResolvedShapeField {
    let author_shape = query_ir::ResolvedShape::new(
        user_type(),
        vec![user_name_shape_field(), user_best_friend_shape_field()],
    );

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    )
}

pub fn optional_post_author_with_best_friend_shape_field() -> query_ir::ResolvedShapeField {
    let author_shape =
        query_ir::ResolvedShape::new(user_type(), vec![user_best_friend_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Optional,
        Some(author_shape),
    )
}

pub fn optional_post_author_with_posts_shape_field() -> query_ir::ResolvedShapeField {
    let posts_shape = query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]);
    let posts = query_ir::ResolvedShapeField::new(
        "posts",
        user_posts_field(),
        schema_model::Cardinality::Many,
        Some(posts_shape),
    );
    let author_shape = query_ir::ResolvedShape::new(user_type(), vec![posts]);

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Optional,
        Some(author_shape),
    )
}

pub fn post_best_friend_shape_field() -> query_ir::ResolvedShapeField {
    let best_friend_shape =
        query_ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "best_friend",
        post_best_friend_field(),
        schema_model::Cardinality::Required,
        Some(best_friend_shape),
    )
}

pub fn optional_post_author_shape_field() -> query_ir::ResolvedShapeField {
    let author_shape = query_ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]);

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Optional,
        Some(author_shape),
    )
}

pub fn post_author_shape_field_with_id_then_name() -> query_ir::ResolvedShapeField {
    let author_shape = query_ir::ResolvedShape::new(
        user_type(),
        vec![user_id_shape_field(), user_name_shape_field()],
    );

    query_ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    )
}

fn user_id_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "id",
        user_id_field(),
        schema_model::Cardinality::Required,
        None,
    )
}

pub fn post_query_with_shape(fields: Vec<query_ir::ResolvedShapeField>) -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), fields),
        None,
        vec![],
        None,
        None,
    )
}

pub fn user_query_with_shape(fields: Vec<query_ir::ResolvedShapeField>) -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        user_type(),
        query_ir::ResolvedShape::new(user_type(), fields),
        None,
        vec![],
        None,
        None,
    )
}
