use schema::{Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog};

pub fn post_only_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new("Post", vec![])])
        .expect("post-only schema catalog should be valid")
}

pub fn post_with_title_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![Field::Scalar(ScalarField::new(
            "title",
            ScalarType::Str,
            schema::SingleCardinality::Required,
        ))],
    )])
    .expect("post-with-title-catalog schema catalog should be valid")
}

pub fn post_with_author_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                schema::SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    schema::SingleCardinality::Required,
                )),
                Field::Link(LinkField::new(
                    "author",
                    "User",
                    schema::Cardinality::Required,
                )),
            ],
        ),
    ])
    .expect("post-with-author schema catalog should be valid")
}
