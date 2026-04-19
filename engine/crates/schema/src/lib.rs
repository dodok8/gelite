use std::collections::HashSet;

const IMPLICIT_ID_FIELD_NAME: &str = "id";
const BUILTIN_SCALAR_TYPE_NAMES: &[&str] =
    &["str", "int64", "float64", "bool", "uuid", "datetime"];

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

impl Field {
    pub fn name(&self) -> &str {
        match self {
            Field::Scalar(scalar) => scalar.name.as_str(),
            Field::Link(link) => link.name.as_str(),
        }
    }

    pub fn cardinality(&self) -> Cardinality {
        match self {
            Field::Scalar(scalar) => match scalar.cardinality {
                SingleCardinality::Optional => Cardinality::Optional,
                SingleCardinality::Required => Cardinality::Required,
            },
            Field::Link(link) => link.cardinality,
        }
    }

    pub fn is_implicit(&self) -> bool {
        match self {
            Field::Scalar(scalar) => scalar.is_implicit,
            Field::Link(_) => false,
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, Field::Scalar(_))
    }

    pub fn is_link(&self) -> bool {
        matches!(self, Field::Link(_))
    }
}

impl ObjectType {
    pub fn new(name: impl Into<String>, declared_fields: Vec<Field>) -> Self {
        Self {
            name: name.into(),
            declared_fields,
            implicit_fields: vec![Field::Scalar(ScalarField {
                name: IMPLICIT_ID_FIELD_NAME.to_string(),
                scalar_type: ScalarType::Uuid,
                cardinality: SingleCardinality::Required,
                is_implicit: true,
            })],
        }
    }

    pub fn find_declared_field(&self, name: &str) -> Option<&Field> {
        self.declared_fields
            .iter()
            .find(|field| field.name() == name)
    }

    pub fn declared_fields(&self) -> &[Field] {
        &self.declared_fields
    }

    pub fn find_field(&self, name: &str) -> Option<&Field> {
        self.implicit_fields
            .iter()
            .find(|field| field.name() == name)
            .or_else(|| self.find_declared_field(name))
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl SchemaCatalog {
    pub fn try_new(object_types: Vec<ObjectType>) -> Result<Self, SchemaError> {
        Self::validate_unique_type_names(&object_types)?;
        Self::validate_unique_field_names_within_type(&object_types)?;
        Self::validate_no_explicit_id_field_declaration(&object_types)?;
        Self::validate_no_unknown_link_target(&object_types)?;
        Self::validate_no_reserved_scalar_type_name_as_object_type_name(&object_types)?;
        Ok(Self { object_types })
    }

    fn validate_unique_type_names(object_types: &[ObjectType]) -> Result<(), SchemaError> {
        let mut seen_types_names = HashSet::new();

        for object_type in object_types {
            let inserted = seen_types_names.insert(object_type.name().to_string());
            if !inserted {
                return Err(SchemaError::DuplicateTypeName {
                    name: object_type.name().to_string(),
                });
            }
        }
        Ok(())
    }

    fn validate_unique_field_names_within_type(
        object_types: &[ObjectType],
    ) -> Result<(), SchemaError> {
        for object_type in object_types {
            let mut seen_field_names = HashSet::new();

            for field in object_type.declared_fields() {
                let field_name = field.name();

                let inserted = seen_field_names.insert(field_name);

                if !inserted {
                    return Err(SchemaError::DuplicateFieldName {
                        object_type: object_type.name().to_string(),
                        field_name: field_name.to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_no_explicit_id_field_declaration(
        object_types: &[ObjectType],
    ) -> Result<(), SchemaError> {
        for object_type in object_types {
            for field in object_type.declared_fields() {
                let field_name = field.name();

                if field_name == IMPLICIT_ID_FIELD_NAME {
                    return Err(SchemaError::ExplicitIdFieldDeclaration {
                        object_type: object_type.name().to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_no_unknown_link_target(object_types: &[ObjectType]) -> Result<(), SchemaError> {
        let known_type_names: HashSet<&str> = object_types
            .iter()
            .map(|object_type| object_type.name())
            .collect();

        for object_type in object_types {
            for field in object_type.declared_fields() {
                let Field::Link(link) = field else {
                    continue;
                };

                let target_type = link.target_type_name.as_str();

                if !known_type_names.contains(target_type) {
                    return Err(SchemaError::UnknownLinkTarget {
                        object_type: object_type.name().to_string(),
                        field_name: field.name().to_string(),
                        target_type: target_type.to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_no_reserved_scalar_type_name_as_object_type_name(
        object_types: &[ObjectType],
    ) -> Result<(), SchemaError> {
        for object_type in object_types {
            let type_name = object_type.name();

            if BUILTIN_SCALAR_TYPE_NAMES.contains(&type_name) {
                return Err(SchemaError::ReservedScalarTypeNameAsObjectTypeName {
                    name: type_name.to_string(),
                });
            }
        }

        Ok(())
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

    pub fn object_types(&self) -> &[ObjectType] {
        &self.object_types
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    DuplicateTypeName {
        name: String,
    },
    DuplicateFieldName {
        object_type: String,
        field_name: String,
    },
    ExplicitIdFieldDeclaration {
        object_type: String,
    },
    UnknownLinkTarget {
        object_type: String,
        field_name: String,
        target_type: String,
    },
    ReservedScalarTypeNameAsObjectTypeName {
        name: String,
    },
}

#[cfg(test)]
mod tests;
