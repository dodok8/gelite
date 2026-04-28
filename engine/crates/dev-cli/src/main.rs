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
            println!("{resolved:#?}");
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
