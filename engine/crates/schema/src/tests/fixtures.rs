use crate::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};

pub fn user_type() -> ObjectType {
    ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField::new(
            "name",
            ScalarType::Str,
            SingleCardinality::Required,
        ))],
    )
}

pub fn book_type() -> ObjectType {
    ObjectType::new(
        "Book",
        vec![
            Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            )),
            Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            Field::Scalar(ScalarField::new(
                "published_at",
                ScalarType::DateTime,
                SingleCardinality::Required,
            )),
        ],
    )
}

pub fn schema_with_user_and_book() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![user_type(), book_type()]).unwrap()
}
