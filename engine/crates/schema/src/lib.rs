#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Str,
    Int64,
    Float64,
    Bool,
    Uuid,
    DateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Scalar(ScalarField),
    Link(LinkField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Optional,
    Required,
    Many,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleCardinality {
    Optional,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarField {
    name: String,
    scalar_type: ScalarType,
    cardinality: SingleCardinality,
    is_implicit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkField {
    name: String,
    target_type_name: String,
    cardinality: Cardinality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectType {
    name: String,
    declared_fields: Vec<Field>,
    implicit_fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCatalog {
    object_types: Vec<ObjectType>,
}

impl ObjectType {
    pub fn new(name: impl Into<String>, declared_fields: Vec<Field>) -> Self {
        Self {
            name: name.into(),
            declared_fields,
            implicit_fields: vec![Field::Scalar(ScalarField {
                name: "id".to_string(),
                scalar_type: ScalarType::Uuid,
                cardinality: SingleCardinality::Required,
                is_implicit: true,
            })],
        }
    }

    pub fn find_declared_field(&self, name: &str) -> Option<&Field> {
        self.declared_fields.iter().find(|field| match field {
            Field::Scalar(scalar) => scalar.name == name,
            Field::Link(link) => link.name == name,
        })
    }

    pub fn declared_fields(&self) -> &[Field] {
        &self.declared_fields
    }

    pub fn find_field(&self, name: &str) -> Option<&Field> {
        self.implicit_fields
            .iter()
            .find(|field| match field {
                Field::Scalar(scalar) => scalar.name == name,
                Field::Link(link) => link.name == name,
            })
            .or_else(|| self.find_declared_field(name))
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl SchemaCatalog {
    pub fn new(object_types: Vec<ObjectType>) -> Self {
        Self { object_types }
    }

    pub fn find_type(&self, name: &str) -> Option<&ObjectType> {
        self.object_types
            .iter()
            .find(|object_type| object_type.name == name)
    }

    pub fn find_field(&self, type_name: &str, field_name: &str) -> Option<&Field> {
        self.find_type(type_name)
            .and_then(|object_type| object_type.find_field(field_name))
    }
}

#[cfg(test)]
mod tests {
    mod object_type_model {

        use crate::{
            Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SingleCardinality,
        };

        #[test]
        fn object_type_exposes_declared_scalar_fields() {
            let user = ObjectType::new(
                "User",
                vec![Field::Scalar(ScalarField {
                    name: "name".to_string(),
                    scalar_type: ScalarType::Str,
                    cardinality: SingleCardinality::Required,
                    is_implicit: false,
                })],
            );

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
            let book = ObjectType::new(
                "Book",
                vec![
                    Field::Link(LinkField {
                        name: "author".to_string(),
                        target_type_name: "User".to_string(),
                        cardinality: Cardinality::Required,
                    }),
                    Field::Scalar(ScalarField {
                        name: "title".to_string(),
                        scalar_type: ScalarType::Str,
                        cardinality: SingleCardinality::Required,
                        is_implicit: false,
                    }),
                ],
            );

            let field = book
                .find_declared_field("author")
                .expect("author field should exist");

            assert!(matches!(field, Field::Link(_)));

            match field {
                Field::Link(link) => {
                    assert_eq!(link.name, "author");
                    assert_eq!(link.target_type_name, "User");
                    assert_eq!(link.cardinality, Cardinality::Required);
                }
                Field::Scalar(_) => {
                    panic!("expected declared field `author` on `Book` to be a link field")
                }
            }
        }

        #[test]
        fn object_type_enumerates_declared_fields_in_definition_order() {
            let book = ObjectType::new(
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
            );
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
                Field::Link(_) => panic!(
                    "expected declared field at index 2 on `Book` to be scalar `published_at`"
                ),
            }
        }

        #[test]
        fn implicit_id_field_exists_on_every_object_type() {
            let user = ObjectType::new(
                "User",
                vec![Field::Scalar(ScalarField {
                    name: "name".to_string(),
                    scalar_type: ScalarType::Str,
                    cardinality: SingleCardinality::Required,
                    is_implicit: false,
                })],
            );

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
            let book = ObjectType::new(
                "Book",
                vec![
                    Field::Link(LinkField {
                        name: "author".to_string(),
                        target_type_name: "User".to_string(),
                        cardinality: Cardinality::Required,
                    }),
                    Field::Scalar(ScalarField {
                        name: "title".to_string(),
                        scalar_type: ScalarType::Str,
                        cardinality: SingleCardinality::Required,
                        is_implicit: false,
                    }),
                ],
            );

            let field = book
                .find_field("author")
                .expect("declared field `author` should be visible through `find_field`");

            match field {
                Field::Link(link) => {
                    assert_eq!(link.name, "author");
                    assert_eq!(link.target_type_name, "User");
                    assert_eq!(link.cardinality, Cardinality::Required);
                }
                Field::Scalar(_) => {
                    panic!("expected visible field `author` on `Book` to be a link field")
                }
            }
        }
    }
    mod catalog_lookup {
        use crate::{
            Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
            SingleCardinality,
        };
        #[test]
        fn catalog_can_lookup_type_by_name() {
            let user = ObjectType::new(
                "User",
                vec![Field::Scalar(ScalarField {
                    name: "name".to_string(),
                    scalar_type: ScalarType::Str,
                    cardinality: SingleCardinality::Required,
                    is_implicit: false,
                })],
            );

            let book = ObjectType::new(
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
                ],
            );

            let schema = SchemaCatalog::new(vec![user, book]);

            let object_type = schema
                .find_type("User")
                .expect("type `User` should be visible through catalog lookup");

            assert_eq!(object_type.name(), "User");
        }

        #[test]
        fn catalog_returns_none_for_unknown_type() {
            let user = ObjectType::new(
                "User",
                vec![Field::Scalar(ScalarField {
                    name: "name".to_string(),
                    scalar_type: ScalarType::Str,
                    cardinality: SingleCardinality::Required,
                    is_implicit: false,
                })],
            );

            let schema = SchemaCatalog::new(vec![user]);

            let missing_type = schema.find_type("Comment");

            assert!(missing_type.is_none());
        }

        #[test]
        fn catalog_can_lookup_field_by_type_and_name() {
            let user = ObjectType::new(
                "User",
                vec![Field::Scalar(ScalarField {
                    name: "name".to_string(),
                    scalar_type: ScalarType::Str,
                    cardinality: SingleCardinality::Required,
                    is_implicit: false,
                })],
            );

            let book = ObjectType::new(
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
                ],
            );

            let schema = SchemaCatalog::new(vec![user, book]);

            let field = schema
                .find_field("Book", "author")
                .expect("field `author` on `Book` should be visible through catalog lookup");

            match field {
                Field::Link(link) => {
                    assert_eq!(link.name, "author");
                    assert_eq!(link.target_type_name, "User");
                    assert_eq!(link.cardinality, Cardinality::Required);
                }
                Field::Scalar(_) => {
                    panic!("expected field `author` on `Book` to be a link field")
                }
            }
        }
    }
    mod validation { /* 3차 */
    }
}
