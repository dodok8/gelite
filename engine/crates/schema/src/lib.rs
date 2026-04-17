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
}

impl ObjectType {
    pub fn new(name: impl Into<String>, declared_fields: Vec<Field>) -> Self {
        Self {
            name: name.into(),
            declared_fields,
        }
    }

    pub fn declared_field(&self, name: &str) -> Option<&Field> {
        self.declared_fields.iter().find(|field| match field {
            Field::Scalar(scalar) => scalar.name == name,
            Field::Link(link) => link.name == name,
        })
    }
}

#[cfg(test)]
mod tests {
    mod object_type_model {

        use crate::{
            Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType,
            SingleCardinality,
        };

        #[test]
        fn exposed_declared_scalar_fields() {
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
                .declared_field("name")
                .expect("name filed should exist");

            assert!(matches!(field, Field::Scalar(_)));

            match field {
                Field::Scalar(scalar) => {
                    assert_eq!(scalar.name, "name");
                    assert_eq!(scalar.cardinality, SingleCardinality::Required);
                    assert_eq!(scalar.scalar_type, ScalarType::Str);
                    assert!(!scalar.is_implicit);
                }
                Field::Link(_) => panic!("expected scalar field"),
            }
        }

        #[test]
        fn exposed_declared_link_fields() {
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
                .declared_field("author")
                .expect("author field should exist");

            assert!(matches!(field, Field::Link(_)));

            match field {
                Field::Link(link) => {
                    assert_eq!(link.name, "author");
                    assert_eq!(link.target_type_name, "User");
                    assert_eq!(link.cardinality, Cardinality::Required);
                }
                Field::Scalar(_) => panic!("expected link field"),
            }
        }
    }
    mod catalog_lookup { /* 2차 */
    }
    mod validation { /* 3차 */
    }
}
