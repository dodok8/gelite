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

pub fn post_or_field() -> FieldRef {
    FieldRef::new(FieldId::new(4), post_type(), "or")
}

pub fn post_quote_field() -> FieldRef {
    FieldRef::new(FieldId::new(5), post_type(), "quote\"field")
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

pub fn post_or_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![query_ir::ResolvedPathStep::scalar(
                post_or_field(),
                schema_model::Cardinality::Required,
            )],
        )
        .expect("post or path should be valid"),
    )
}

pub fn post_quote_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![query_ir::ResolvedPathStep::scalar(
                post_quote_field(),
                schema_model::Cardinality::Required,
            )],
        )
        .expect("post quote path should be valid"),
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

pub fn post_id_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), post_type(), "id")
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

pub fn post_author_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), post_type(), "author")
}

pub fn post_view_count_field() -> FieldRef {
    FieldRef::new(FieldId::new(6), post_type(), "view_count")
}

pub fn post_rating_field() -> FieldRef {
    FieldRef::new(FieldId::new(7), post_type(), "rating")
}

pub fn post_published_field() -> FieldRef {
    FieldRef::new(FieldId::new(8), post_type(), "published")
}

pub fn post_subtitle_field() -> FieldRef {
    FieldRef::new(FieldId::new(9), post_type(), "subtitle")
}

pub fn empty_post_insert_query() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(post_type(), vec![])
}

fn scalar_assignment(field: FieldRef, literal: query_ir::Literal) -> query_ir::Assignment {
    query_ir::Assignment::new(field, query_ir::AssignmentValue::Scalar(literal))
}

pub fn post_insert_with_scalar_assignments() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![
            scalar_assignment(
                post_title_field(),
                query_ir::Literal::String("Case File".to_string()),
            ),
            scalar_assignment(post_view_count_field(), query_ir::Literal::Int64(7)),
            scalar_assignment(post_rating_field(), query_ir::Literal::Float64(4.5)),
            scalar_assignment(post_published_field(), query_ir::Literal::Bool(true)),
        ],
    )
}

pub fn post_insert_with_author_link() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![query_ir::Assignment::new(
            post_author_field(),
            query_ir::AssignmentValue::LinkId("00000000-0000-0000-0000-000000000001".to_string()),
        )],
    )
}

pub fn post_author_select_assignment() -> query_ir::Assignment {
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

    query_ir::Assignment::new(
        post_author_field(),
        query_ir::AssignmentValue::LinkSelect(alloc::boxed::Box::new(select)),
    )
}

pub fn post_insert_with_author_select() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(post_type(), vec![post_author_select_assignment()])
}

pub fn post_insert_with_null_assignments() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![
            query_ir::Assignment::new(post_subtitle_field(), query_ir::AssignmentValue::ScalarNull),
            query_ir::Assignment::new(post_author_field(), query_ir::AssignmentValue::LinkNull),
        ],
    )
}

pub fn post_insert_with_ordered_assignments() -> query_ir::InsertQuery {
    query_ir::InsertQuery::new(
        post_type(),
        vec![
            scalar_assignment(post_view_count_field(), query_ir::Literal::Int64(7)),
            scalar_assignment(
                post_title_field(),
                query_ir::Literal::String("Case File".to_string()),
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

pub fn quoted_insert_query() -> query_ir::InsertQuery {
    let object_type = ObjectTypeRef::new(ObjectTypeId::new(3), "Post\"Archive");
    let title = FieldRef::new(FieldId::new(2), object_type.clone(), "title\"quote");

    query_ir::InsertQuery::new(
        object_type,
        vec![scalar_assignment(
            title,
            query_ir::Literal::String("Case File".to_string()),
        )],
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

pub fn post_title_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema_model::Cardinality::Required,
        None,
    )
}

pub fn post_or_shape_field() -> query_ir::ResolvedShapeField {
    query_ir::ResolvedShapeField::new(
        "or",
        post_or_field(),
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

pub fn user_score_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), user_type(), "score")
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

pub fn post_query_with_filter(filter: query_ir::Expr) -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        Some(filter),
        vec![],
        None,
        None,
    )
}

pub fn post_query_with_order_by(order_by: Vec<query_ir::OrderExpr>) -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        order_by,
        None,
        None,
    )
}

pub fn post_query_with_limit_and_offset(limit: i64, offset: i64) -> query_ir::SelectQuery {
    query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![],
        Some(limit),
        Some(offset),
    )
}
