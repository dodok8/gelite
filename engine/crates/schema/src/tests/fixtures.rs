use crate::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};

pub fn user_type() -> ObjectType {
    ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField {
            name: "name".to_string(),
            scalar_type: ScalarType::Str,
            cardinality: SingleCardinality::Required,
            is_implicit: false,
        })],
    )
}

pub fn book_type() -> ObjectType {
    ObjectType::new(
        "Book",
        vec![
            Field::Scalar(ScalarField {
                name: "title".to_string(),
                scalar_type: ScalarType::Str,
                cardinality: SingleCardinality::Required,
                is_implicit: false,
            }),
            Field::Link(LinkField {
                name: "author".to_string(),
                target_type_name: "User".to_string(),
                cardinality: Cardinality::Required,
            }),
            Field::Scalar(ScalarField {
                name: "published_at".to_string(),
                scalar_type: ScalarType::DateTime,
                cardinality: SingleCardinality::Required,
                is_implicit: false,
            }),
        ],
    )
}

pub fn schema_with_user_and_book() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![user_type(), book_type()]).unwrap()
}
