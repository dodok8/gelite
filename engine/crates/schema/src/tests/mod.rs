mod fixtures;

use crate::{
    Field, ObjectType, ScalarField, ScalarType, SchemaCatalog, SchemaError, SingleCardinality,
};
use fixtures::{book_type, schema_with_user_and_book, user_type};

#[test]
fn object_type_exposes_declared_scalar_fields() {
    let user = user_type();

    let field = user
        .find_declared_field("name")
        .expect("name field should exist");

    assert!(matches!(field, Field::Scalar(_)));

    match field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.name, "name");
            assert_eq!(scalar.cardinality, SingleCardinality::Required);
            assert_eq!(scalar.scalar_type, ScalarType::Str);
            assert!(!scalar.is_implicit);
        }
        Field::Link(_) => {
            panic!("expected declared field `name` on `User` to be a scalar field")
        }
    }
}

#[test]
fn object_type_exposes_declared_link_fields() {
    let book = book_type();

    let field = book
        .find_declared_field("author")
        .expect("author field should exist");

    assert!(matches!(field, Field::Link(_)));

    match field {
        Field::Link(link) => {
            assert_eq!(link.name, "author");
            assert_eq!(link.target_type_name, "User");
            assert_eq!(link.cardinality, crate::Cardinality::Required);
        }
        Field::Scalar(_) => {
            panic!("expected declared field `author` on `Book` to be a link field")
        }
    }
}

#[test]
fn object_type_enumerates_declared_fields_in_definition_order() {
    let book = book_type();
    let fields = book.declared_fields();

    assert_eq!(fields.len(), 3);

    match &fields[0] {
        Field::Scalar(scalar) => assert_eq!(scalar.name, "title"),
        Field::Link(_) => {
            panic!("expected declared field at index 0 on `Book` to be scalar `title`")
        }
    }

    match &fields[1] {
        Field::Link(link) => assert_eq!(link.name, "author"),
        Field::Scalar(_) => {
            panic!("expected declared field at index 1 on `Book` to be link `author`")
        }
    }

    match &fields[2] {
        Field::Scalar(scalar) => assert_eq!(scalar.name, "published_at"),
        Field::Link(_) => {
            panic!("expected declared field at index 2 on `Book` to be scalar `published_at`")
        }
    }
}

#[test]
fn implicit_id_field_exists_on_every_object_type() {
    let user = user_type();

    let id_field = user
        .find_field("id")
        .expect("implicit field `id` should exist on every object type");

    match id_field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.name, "id");
            assert_eq!(scalar.scalar_type, ScalarType::Uuid);
            assert_eq!(scalar.cardinality, SingleCardinality::Required);
            assert!(scalar.is_implicit);
        }
        Field::Link(_) => {
            panic!("expected implicit field `id` on `User` to be a scalar field")
        }
    }
}

#[test]
fn find_field_returns_declared_fields_as_well() {
    let book = book_type();

    let field = book
        .find_field("author")
        .expect("declared field `author` should be visible through `find_field`");

    match field {
        Field::Link(link) => {
            assert_eq!(link.name, "author");
            assert_eq!(link.target_type_name, "User");
            assert_eq!(link.cardinality, crate::Cardinality::Required);
        }
        Field::Scalar(_) => {
            panic!("expected visible field `author` on `Book` to be a link field")
        }
    }
}

#[test]
fn catalog_can_lookup_type_by_name() {
    let schema = schema_with_user_and_book();

    let object_type = schema
        .find_type("User")
        .expect("type `User` should be visible through catalog lookup");

    assert_eq!(object_type.name(), "User");
}

#[test]
fn catalog_returns_none_for_unknown_type() {
    let schema = SchemaCatalog::try_new(vec![user_type()]).unwrap();

    let missing_type = schema.find_type("Comment");

    assert!(missing_type.is_none());
}

#[test]
fn catalog_can_lookup_field_by_type_and_name() {
    let schema = schema_with_user_and_book();

    let field = schema
        .find_field("Book", "author")
        .expect("field `author` on `Book` should be visible through catalog lookup");

    match field {
        Field::Link(link) => {
            assert_eq!(link.name, "author");
            assert_eq!(link.target_type_name, "User");
            assert_eq!(link.cardinality, crate::Cardinality::Required);
        }
        Field::Scalar(_) => {
            panic!("expected field `author` on `Book` to be a link field")
        }
    }
}

#[test]
fn catalog_returns_none_for_unknown_field() {
    let schema = schema_with_user_and_book();

    let field = schema.find_field("Book", "isbn");
    assert!(field.is_none())
}

#[test]
fn catalog_field_lookup_can_find_implicit_id() {
    let schema = schema_with_user_and_book();

    let id_field = schema
        .find_field("Book", "id")
        .expect("implicit field `id` should exist on every object type");

    match id_field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.name, "id");
            assert_eq!(scalar.scalar_type, ScalarType::Uuid);
            assert_eq!(scalar.cardinality, SingleCardinality::Required);
            assert!(scalar.is_implicit);
        }
        Field::Link(_) => {
            panic!("expected implicit field `id` on `User` to be a scalar field")
        }
    }
}

#[test]
fn catalog_returns_none_for_unknown_type_when_looking_up_field() {
    let schema = schema_with_user_and_book();

    let id_field = schema.find_field("Comment", "id");
    assert!(id_field.is_none())
}

#[test]
fn catalog_preserves_type_iteration_order() {
    let schema = SchemaCatalog::try_new(vec![user_type(), book_type()]).unwrap();
    let object_types = schema.object_types();
    assert_eq!(object_types[0].name(), "User");
    assert_eq!(object_types[1].name(), "Book");
}

#[test]
fn rejects_duplicate_type_names() {
    let user1 = user_type();
    let user2 = user_type();

    let result = SchemaCatalog::try_new(vec![user1, user2]);
    assert_eq!(
        result,
        Err(SchemaError::DuplicateTypeName {
            name: "User".to_string()
        })
    );
}

#[test]
fn rejects_duplicate_field_names_within_type() {
    let user = ObjectType::new(
        "User",
        vec![
            Field::Scalar(ScalarField {
                name: "name".to_string(),
                scalar_type: ScalarType::Str,
                cardinality: SingleCardinality::Optional,
                is_implicit: false,
            }),
            Field::Scalar(ScalarField {
                name: "name".to_string(),
                scalar_type: ScalarType::Str,
                cardinality: SingleCardinality::Optional,
                is_implicit: false,
            }),
        ],
    );

    let result = SchemaCatalog::try_new(vec![user]);

    assert_eq!(
        result,
        Err(SchemaError::DuplicateFieldName {
            object_type: "User".to_string(),
            field_name: "name".to_string()
        })
    )
}

#[test]
fn rejects_explicit_id_field_declaration() {
    let user = ObjectType::new(
        "User",
        vec![
            Field::Scalar(ScalarField {
                name: "name".to_string(),
                scalar_type: ScalarType::Str,
                cardinality: SingleCardinality::Optional,
                is_implicit: false,
            }),
            Field::Scalar(ScalarField {
                name: "id".to_string(),
                scalar_type: ScalarType::Int64,
                cardinality: SingleCardinality::Optional,
                is_implicit: false,
            }),
        ],
    );

    let result = SchemaCatalog::try_new(vec![user]);

    assert_eq!(
        result,
        Err(SchemaError::ExplicitIdFieldDeclaration {
            object_type: "User".to_string(),
        })
    )
}

#[test]
fn rejects_unknown_link_target() {
    let book = book_type();

    let result = SchemaCatalog::try_new(vec![book]);

    assert_eq!(
        result,
        Err(SchemaError::UnknownLinkTarget {
            object_type: "Book".to_string(),
            field_name: "author".to_string(),
            target_type: "User".to_string()
        })
    )
}
