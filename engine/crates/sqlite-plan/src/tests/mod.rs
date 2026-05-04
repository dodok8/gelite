mod fixtures;

use crate::{
    SQLiteCompareOp, SQLiteJoinKind, SQLiteJoinReason, SQLiteOrderDirection, SQLiteValueExpr,
    SQLiteValueRole, SQLiteWhereExpr, plan_select,
};
use fixtures::{
    empty_post_query, post_author_field, post_author_shape_field, post_id_field,
    post_id_shape_field, post_query_with_shape, post_title_field, post_title_shape_field,
    post_type,
};
use ir::{Literal, ResolvedShape, ResolvedShapeField, SelectQuery};

#[test]
fn sqlite_select_plan_can_store_root_source() {
    let ir = empty_post_query();

    let plan = plan_select(&ir);

    assert_eq!(plan.root_source().object_type().name(), "Post");
    assert_eq!(plan.root_source().table_name(), "post");
    assert_eq!(plan.root_source().alias(), "root");
    assert_eq!(plan.root_source().id_column(), "id");
}

#[test]
fn sqlite_select_plan_can_project_root_scalar_field() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_eq!(selected_values[0].source_alias(), "root");
    assert_eq!(selected_values[0].column_name(), "title");
    assert_eq!(selected_values[0].output_name(), "title");
    assert_eq!(selected_values[0].field().name(), "title");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_preserves_root_scalar_output_name() {
    let title = ResolvedShapeField::new(
        "headline",
        post_title_field(),
        schema::Cardinality::Required,
        None,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![title]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_eq!(selected_values[0].source_alias(), "root");
    assert_eq!(selected_values[0].column_name(), "title");
    assert_eq!(selected_values[0].output_name(), "headline");
    assert_eq!(selected_values[0].field().name(), "title");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_preserves_root_scalar_projection_order() {
    let title = ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema::Cardinality::Required,
        None,
    );

    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema::Cardinality::Required,
        None,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![title, author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);

    assert_eq!(selected_values[0].source_alias(), "root");
    assert_eq!(selected_values[0].column_name(), "title");
    assert_eq!(selected_values[0].output_name(), "title");
    assert_eq!(selected_values[0].field().name(), "title");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Scalar);

    assert_eq!(selected_values[1].source_alias(), "root");
    assert_eq!(selected_values[1].column_name(), "author");
    assert_eq!(selected_values[1].output_name(), "author");
    assert_eq!(selected_values[1].field().name(), "author");
    assert_eq!(selected_values[1].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_can_apply_limit() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        Some(10),
        None,
    );

    let plan = plan_select(&ir);

    assert_eq!(plan.limit(), Some(10));
}

#[test]
fn sqlite_select_plan_can_apply_offset() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        Some(20),
    );

    let plan = plan_select(&ir);

    assert_eq!(plan.offset(), Some(20));
}

#[test]
fn sqlite_select_plan_can_order_by_root_scalar_field() {
    let order_by = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_title_field()),
        ir::OrderDirection::Asc,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].source_alias(), "root");
    assert_eq!(order_by[0].column_name(), "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
}

#[test]
fn sqlite_select_plan_can_order_by_root_scalar_field_desc() {
    let order_by = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_title_field()),
        ir::OrderDirection::Desc,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].source_alias(), "root");
    assert_eq!(order_by[0].column_name(), "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Desc);
}

#[test]
fn sqlite_select_plan_preserves_order_by_order() {
    let title_order = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_title_field()),
        ir::OrderDirection::Asc,
    );

    let author_order = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_author_field()),
        ir::OrderDirection::Desc,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![title_order, author_order],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 2);
    assert_eq!(order_by[0].column_name(), "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
    assert_eq!(order_by[1].column_name(), "author");
    assert_eq!(order_by[1].direction(), SQLiteOrderDirection::Desc);
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_equals_string_literal() {
    let filter = ir::CompareExpr::new(
        ir::ValueExpr::Field(post_title_field()),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    );

    let expr = ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            match compare.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "root");
                    assert_eq!(column.column_name(), "title");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
            }

            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(crate::SQLiteLiteral::String(value)) => {
                    assert_eq!(value, "hello");
                }
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
            }
        }
        None => panic!("Expected Some Filter!"),
    }
}

#[test]
fn sqlite_select_plan_preserves_absent_filter() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    assert!(plan.filter().is_none());
}

#[test]
fn sqlite_select_plan_can_filter_implicit_id_equals_string_literal() {
    let filter = ir::CompareExpr::new(
        ir::ValueExpr::Field(post_id_field()),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    );

    let expr = ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            match compare.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "root");
                    assert_eq!(column.column_name(), "id");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
            }

            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(crate::SQLiteLiteral::String(value)) => {
                    assert_eq!(value, "hello");
                }
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
            }
        }
        None => panic!("Expected Some Filter!"),
    }
}

#[test]
fn sqlite_select_plan_can_order_by_implicit_id() {
    let order_by = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_id_field()),
        ir::OrderDirection::Asc,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].source_alias(), "root");
    assert_eq!(order_by[0].column_name(), "id");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
}

#[test]
fn sqlite_select_plan_can_project_implicit_id() {
    let ir = post_query_with_shape(vec![post_id_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_eq!(selected_values[0].source_alias(), "root");
    assert_eq!(selected_values[0].column_name(), "id");
    assert_eq!(selected_values[0].output_name(), "id");
    assert_eq!(selected_values[0].field().name(), "id");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::ObjectId);
}

#[test]
fn sqlite_select_plan_can_join_selected_single_link() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");

    let on = joins[0].on();

    assert_eq!(on.left_alias(), "root");
    assert_eq!(on.left_column(), "author_id");
    assert_eq!(on.right_alias(), "author");
    assert_eq!(on.right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::SelectedSingleLink { field } => {
            assert_eq!(field.name(), "author");
        }
    }
}

#[test]
fn sqlite_select_plan_can_project_selected_single_link_scalar_field() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values[1].source_alias(), "author");
    assert_eq!(selected_values[1].column_name(), "name");
    assert_eq!(selected_values[1].output_name(), "name");
    assert_eq!(selected_values[1].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_projects_selected_single_link_identity() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);

    assert_eq!(selected_values[0].source_alias(), "author");
    assert_eq!(selected_values[0].column_name(), "id");
    assert_eq!(selected_values[0].output_name(), "id");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::ObjectId);

    assert_eq!(selected_values[1].source_alias(), "author");
    assert_eq!(selected_values[1].column_name(), "name");
    assert_eq!(selected_values[1].output_name(), "name");
    assert_eq!(selected_values[1].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_preserves_selected_value_order_with_nested_link() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 3);

    assert_eq!(selected_values[0].source_alias(), "root");
    assert_eq!(selected_values[0].column_name(), "title");
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Scalar);

    assert_eq!(selected_values[1].source_alias(), "author");
    assert_eq!(selected_values[1].column_name(), "id");
    assert_eq!(selected_values[1].role(), SQLiteValueRole::ObjectId);

    assert_eq!(selected_values[2].source_alias(), "author");
    assert_eq!(selected_values[2].column_name(), "name");
    assert_eq!(selected_values[2].role(), SQLiteValueRole::Scalar);
}

#[test]
fn sqlite_select_plan_can_build_result_shape_for_root_scalar_fields() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);
    let plan = plan_select(&ir);
    let result_shape = plan.result_shape();

    let fields = result_shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].cardinality(), schema::Cardinality::Required);

    let value = fields[0]
        .value()
        .expect("title should point to a selected value");

    assert_eq!(value.source_alias(), "root");
    assert_eq!(value.column_name(), "title");
    assert_eq!(value.role(), SQLiteValueRole::Scalar);

    assert!(fields[0].nested_shape().is_none());
}

#[test]
fn sqlite_select_plan_can_build_result_shape_for_selected_single_link() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);
    let result_shape = plan.result_shape();

    let fields = result_shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].cardinality(), schema::Cardinality::Required);
    assert!(fields[0].value().is_none());

    let nested_shape = fields[0]
        .nested_shape()
        .expect("author should have nested result shape");

    let nested_fields = nested_shape.fields();

    assert_eq!(nested_fields.len(), 1);
    assert_eq!(nested_fields[0].output_name(), "name");
    assert_eq!(
        nested_fields[0].cardinality(),
        schema::Cardinality::Required
    );

    let value = nested_fields[0]
        .value()
        .expect("name should point to a selected value");

    assert_eq!(value.source_alias(), "author");
    assert_eq!(value.column_name(), "name");
    assert_eq!(value.role(), SQLiteValueRole::Scalar);
    assert!(nested_fields[0].nested_shape().is_none());
}

#[test]
fn sqlite_result_shape_for_selected_single_link_has_identity_value() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);

    let author = &plan.result_shape().fields()[0];
    let nested_shape = author
        .nested_shape()
        .expect("author should have nested result shape");

    let identity = nested_shape
        .identity_value()
        .expect("nested shape should have identity value");

    assert_eq!(identity.source_alias(), "author");
    assert_eq!(identity.column_name(), "id");
    assert_eq!(identity.role(), SQLiteValueRole::ObjectId);
}
