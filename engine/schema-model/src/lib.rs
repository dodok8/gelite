#![no_std]
//! Schema catalog types for the Gelite MVP.
//!
//! This crate models the semantic schema after parsing and validation. It is
//! deliberately independent from the query parser, resolver, SQLite planner,
//! and runtime. The resolver consumes [`SchemaCatalog`] as the source of truth
//! for object types, scalar fields, declared links, field cardinality, and the
//! implicit `id` field that exists on every object type.
//!
//! The current catalog is built directly from Rust values. A later schema
//! parser can produce the same structures, but the invariants should stay here:
//! type names are unique, field names are unique within an object type, `id`
//! cannot be declared explicitly, link targets must exist, and built-in scalar
//! type names cannot be reused as object type names.
//!
//! The catalog keeps definition order stable. The generated object and field
//! references are deterministic within one catalog: object ids are derived from
//! object definition order, and field ids are derived from implicit fields
//! followed by declared fields.

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const IMPLICIT_ID_FIELD_NAME: &str = "id";
const BUILTIN_SCALAR_TYPE_NAMES: &[&str] = &["str", "int64", "float64", "bool", "uuid", "datetime"];

/// Built-in scalar types supported by the schema MVP.
///
/// These names are reserved for scalar fields and cannot be used as object type
/// names. The SQLite storage spec maps them to fixed SQLite affinities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Str,
    Int64,
    Float64,
    Bool,
    Uuid,
    DateTime,
}

/// A field declared on an object type, or an implicit scalar field exposed by
/// the catalog.
///
/// A [`Field::Scalar`] stores a value on the owning object. A [`Field::Link`]
/// stores a relation to another object type and is the only kind of field that
/// can be traversed by query paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Scalar(ScalarField),
    Link(LinkField),
}

/// Cardinality used by query resolution and IR.
///
/// Scalar fields can only be [`Cardinality::Optional`] or
/// [`Cardinality::Required`]. Link fields may also be [`Cardinality::Many`],
/// which represents an unordered collection in the MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Optional,
    Required,
    Many,
}

/// Cardinality for scalar fields, where `multi` is intentionally impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleCardinality {
    Optional,
    Required,
}

/// Uniqueness constraint for scalar fields.
///
/// `Unique` rejects duplicate present values. Optional unique fields may still
/// have multiple absent values; SQLite stores those as multiple `NULL` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uniqueness {
    NotUnique,
    Unique,
}

/// A scalar field stored as a direct value on the owning object.
///
/// The implicit `id` field is represented as a scalar field with
/// [`ScalarType::Uuid`], required cardinality, and `is_implicit = true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarField {
    name: String,
    scalar_type: ScalarType,
    cardinality: SingleCardinality,
    uniqueness: Uniqueness,
    is_implicit: bool,
}

/// A declared schema link from one object type to another.
///
/// The target is stored by name in the catalog input so validation can reject
/// unknown targets before query resolution starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkField {
    name: String,
    target_type_name: String,
    cardinality: Cardinality,
    uniqueness: Uniqueness,
    inverse_field_name: Option<String>,
}

/// An object type with declared fields and catalog-injected implicit fields.
///
/// [`ObjectType::declared_fields`] returns only fields written by the schema
/// author. [`ObjectType::find_field`] also sees implicit fields such as `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectType {
    name: String,
    declared_fields: Vec<Field>,
    implicit_fields: Vec<Field>,
}

/// Deterministic object type identifier within a [`SchemaCatalog`].
///
/// The value uses `i64` because SQLite stores catalog metadata ids as signed
/// 64-bit integers. Keeping the schema id type aligned with SQLite avoids a
/// fallible `u64` to `i64` conversion in backend metadata planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectTypeId(i64);

/// Resolved reference to an object type.
///
/// Query IR uses this instead of raw type names so later compiler stages do not
/// need to repeat name lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectTypeRef {
    id: ObjectTypeId,
    name: String,
}

/// Deterministic field identifier within the owning object type.
///
/// The value uses `i64` for the same reason as [`ObjectTypeId`]: field ids are
/// persisted in SQLite catalog tables as signed 64-bit integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(i64);

/// Resolved reference to a field on a specific object type.
///
/// The same field name on two different object types yields different
/// references because the owner object reference is part of the value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldRef {
    id: FieldId,
    owner_object_type: ObjectTypeRef,
    name: String,
}

/// Validated semantic schema catalog used by the resolver.
///
/// Construction goes through [`SchemaCatalog::try_new`] so invalid schema
/// shapes are rejected before the query pipeline sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCatalog {
    object_types: Vec<ObjectType>,
}

impl ObjectTypeId {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(self) -> i64 {
        self.0
    }
}

impl ObjectTypeRef {
    pub fn new(id: ObjectTypeId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    pub fn id(&self) -> ObjectTypeId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl FieldId {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(self) -> i64 {
        self.0
    }
}

impl FieldRef {
    pub fn new(id: FieldId, owner_object_type: ObjectTypeRef, name: impl Into<String>) -> Self {
        Self {
            id,
            owner_object_type,
            name: name.into(),
        }
    }

    pub fn id(&self) -> FieldId {
        self.id
    }

    pub fn owner_object_type(&self) -> &ObjectTypeRef {
        &self.owner_object_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ScalarField {
    /// Creates a declared scalar field.
    ///
    /// This constructor cannot create the implicit `id` field. The catalog adds
    /// that field when each [`ObjectType`] is created.
    pub fn new(
        name: impl Into<String>,
        scalar_type: ScalarType,
        cardinality: SingleCardinality,
    ) -> Self {
        Self::with_uniqueness(name, scalar_type, cardinality, Uniqueness::NotUnique)
    }

    pub fn with_uniqueness(
        name: impl Into<String>,
        scalar_type: ScalarType,
        cardinality: SingleCardinality,
        uniqueness: Uniqueness,
    ) -> Self {
        Self {
            name: name.into(),
            scalar_type,
            cardinality,
            uniqueness,
            is_implicit: false,
        }
    }

    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }

    pub fn uniqueness(&self) -> Uniqueness {
        self.uniqueness
    }

    pub fn is_unique(&self) -> bool {
        self.uniqueness == Uniqueness::Unique
    }
}

impl LinkField {
    /// Creates a declared link field.
    ///
    /// The target type name is validated when the containing objects are put
    /// into a [`SchemaCatalog`].
    pub fn new(
        name: impl Into<String>,
        target_type_name: impl Into<String>,
        cardinality: Cardinality,
    ) -> Self {
        Self::with_uniqueness(name, target_type_name, cardinality, Uniqueness::NotUnique)
    }

    pub fn with_uniqueness(
        name: impl Into<String>,
        target_type_name: impl Into<String>,
        cardinality: Cardinality,
        uniqueness: Uniqueness,
    ) -> Self {
        Self {
            name: name.into(),
            target_type_name: target_type_name.into(),
            cardinality,
            uniqueness,
            inverse_field_name: None,
        }
    }

    /// Declares a readonly inverse of a stored field on the target type.
    /// The catalog validates the source field, target, and many cardinality.
    pub fn with_inverse(
        name: impl Into<String>,
        target_type_name: impl Into<String>,
        cardinality: Cardinality,
        inverse_field_name: impl Into<String>,
    ) -> Self {
        let mut link = Self::new(name, target_type_name, cardinality);
        link.inverse_field_name = Some(inverse_field_name.into());
        link
    }

    pub fn inverse_field_name(&self) -> Option<&str> {
        self.inverse_field_name.as_deref()
    }

    pub fn target_type_name(&self) -> &str {
        &self.target_type_name
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }

    pub fn uniqueness(&self) -> Uniqueness {
        self.uniqueness
    }

    pub fn is_unique(&self) -> bool {
        self.uniqueness == Uniqueness::Unique
    }
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
    /// Creates an object type and injects the implicit required UUID `id` field.
    pub fn new(name: impl Into<String>, declared_fields: Vec<Field>) -> Self {
        Self {
            name: name.into(),
            declared_fields,
            implicit_fields: vec![Field::Scalar(ScalarField {
                name: IMPLICIT_ID_FIELD_NAME.to_string(),
                scalar_type: ScalarType::Uuid,
                cardinality: SingleCardinality::Required,
                uniqueness: Uniqueness::NotUnique,
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
    /// Builds a catalog and validates the schema invariants enforced today.
    pub fn try_new(object_types: Vec<ObjectType>) -> Result<Self, SchemaError> {
        Self::validate_unique_type_names(&object_types)?;
        Self::validate_unique_field_names_within_type(&object_types)?;
        Self::validate_no_explicit_id_field_declaration(&object_types)?;
        Self::validate_no_unknown_link_target(&object_types)?;
        Self::validate_no_reserved_scalar_type_name_as_object_type_name(&object_types)?;
        Self::validate_no_unique_many_link_field(&object_types)?;
        Self::validate_inverse_links(&object_types)?;
        Ok(Self { object_types })
    }

    fn validate_inverse_links(object_types: &[ObjectType]) -> Result<(), SchemaError> {
        for object in object_types {
            for field in object.declared_fields() {
                let Field::Link(link) = field else { continue };
                let Some(source_name) = link.inverse_field_name() else {
                    continue;
                };
                let invalid = |reason| SchemaError::InvalidInverseLink {
                    object_type: object.name().to_string(),
                    field_name: field.name().to_string(),
                    reason,
                };
                if link.cardinality() != Cardinality::Many {
                    return Err(invalid("inverse links must have many cardinality"));
                }
                let source = object_types
                    .iter()
                    .find(|target| target.name() == link.target_type_name())
                    .and_then(|target| target.find_field(source_name));
                let Some(Field::Link(source)) = source else {
                    return Err(invalid(
                        "inverse source must name a stored link on the target type",
                    ));
                };
                if source.inverse_field_name().is_some() {
                    return Err(invalid("inverse source cannot be another inverse link"));
                }
                if source.target_type_name() != object.name() {
                    return Err(invalid(
                        "inverse source must target the declaring object type",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_unique_type_names(object_types: &[ObjectType]) -> Result<(), SchemaError> {
        let mut seen_types_names = BTreeSet::new();

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
            let mut seen_field_names = BTreeSet::new();

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
        let known_type_names: BTreeSet<&str> = object_types
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

    fn validate_no_unique_many_link_field(object_types: &[ObjectType]) -> Result<(), SchemaError> {
        for object_type in object_types {
            for field in object_type.declared_fields() {
                let Field::Link(link) = field else {
                    continue;
                };

                if link.cardinality() == Cardinality::Many && link.is_unique() {
                    return Err(SchemaError::UniqueManyLinkField {
                        object_type: object_type.name().to_string(),
                        field_name: field.name().to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn find_type(&self, name: &str) -> Option<&ObjectType> {
        self.object_types
            .iter()
            .find(|object_type| object_type.name == name)
    }

    pub fn find_type_ref(&self, name: &str) -> Option<ObjectTypeRef> {
        self.object_types
            .iter()
            .position(|object_type| object_type.name == name)
            .map(|index| {
                let object_type = &self.object_types[index];
                ObjectTypeRef::new(ObjectTypeId::new((index + 1) as i64), object_type.name())
            })
    }

    pub fn find_field(&self, type_name: &str, field_name: &str) -> Option<&Field> {
        self.find_type(type_name)
            .and_then(|object_type| object_type.find_field(field_name))
    }

    pub fn find_field_ref(&self, type_name: &str, field_name: &str) -> Option<FieldRef> {
        let object_type_index = self
            .object_types
            .iter()
            .position(|object_type| object_type.name == type_name)?;
        let object_type = &self.object_types[object_type_index];
        let field_index = object_type
            .implicit_fields
            .iter()
            .chain(object_type.declared_fields.iter())
            .position(|field| field.name() == field_name)?;

        Some(FieldRef::new(
            FieldId::new((field_index + 1) as i64),
            ObjectTypeRef::new(
                ObjectTypeId::new((object_type_index + 1) as i64),
                object_type.name(),
            ),
            field_name,
        ))
    }

    pub fn object_types(&self) -> &[ObjectType] {
        &self.object_types
    }
}

/// Validation errors reported while constructing a [`SchemaCatalog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    InvalidInverseLink {
        object_type: String,
        field_name: String,
        reason: &'static str,
    },
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
    UniqueManyLinkField {
        object_type: String,
        field_name: String,
    },
}

#[cfg(test)]
mod tests;
