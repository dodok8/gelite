use query_ast::{
    CompareExpr, CompareOp, Expr, Literal, OrderDirection, OrderExpr, Path, PathStep, SelectQuery,
    Shape, ShapeItem,
};
use schema::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};

fn main() {
    let catalog = build_schema();
    let query = build_query();

    match resolver::resolve_select(&catalog, &query) {
        Ok(resolved) => {
            let plan = sqlite_plan::plan_select(&resolved);
            let statement = sqlite_sqlgen::render_select(&plan);

            println!("Resolved IR:\n{resolved:#?}");
            println!(
                "SQLite Plan:\n  root: {} as {}\n  selected values:",
                plan.root_source().table_name(),
                plan.root_source().alias()
            );
            for value in plan.selected_values() {
                println!(
                    "    {}.{} -> {}",
                    value.source_alias(),
                    value.column_name(),
                    value.output_name()
                );
            }
            println!("  joins:");
            for join in plan.joins() {
                let on = join.on();
                let join_kind = match join.kind() {
                    sqlite_plan::SQLiteJoinKind::Inner => "inner join",
                    sqlite_plan::SQLiteJoinKind::Left => "left join",
                };
                println!(
                    "    {} {} as {} on {}.{} = {}.{}",
                    join_kind,
                    join.target_table(),
                    join.target_alias(),
                    on.left_alias(),
                    on.left_column(),
                    on.right_alias(),
                    on.right_column()
                );
            }
            println!("SQL:\n{}", statement.sql());
        }
        Err(error) => {
            eprintln!("failed to resolve query: {error:#?}");
            std::process::exit(1);
        }
    }
}

fn build_schema() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .expect("hardcoded development schema should be valid")
}

fn build_query() -> SelectQuery {
    let author_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let shape = Shape::new(vec![
        ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
        ShapeItem::new(Path::new(vec![PathStep::new("author")]), Some(author_shape)),
    ]);

    let filter = Expr::Compare(CompareExpr::new(
        Path::new(vec![PathStep::new("title")]),
        CompareOp::Eq,
        Literal::String("Hello".to_string()),
    ));

    let order = OrderExpr::new(
        Path::new(vec![PathStep::new("title")]),
        OrderDirection::Desc,
    );

    SelectQuery::new("Post", shape, Some(filter), vec![order], Some(10), Some(0))
}
