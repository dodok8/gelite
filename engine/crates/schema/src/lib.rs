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
    pub fn try_new(object_types: Vec<ObjectType>) -> Result<Self, SchemaError> {
        Ok(Self { object_types })
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
    DuplicateTypeName { name: String },
}

#[cfg(test)]
mod tests;
