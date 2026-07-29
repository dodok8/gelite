#![no_std]
//! Backend-independent Semantic IR for resolved queries.
//!
//! The IR is the boundary between language semantics and backend-specific
//! execution planning. It contains resolved schema references, result shapes,
//! path steps, expression nodes, and cardinality metadata, but it does not know
//! SQLite table names, column names, join aliases, or SQL syntax.
//!
//! The resolver builds these types from `query-ast` and `schema`. Downstream
//! crates such as `sqlite-query-plan` must be able to lower the IR without looking
//! back at raw query text.
//!
//! The implemented subset currently represents `select`, `insert`, and
//! `update` queries. Delete is part of the MVP query spec but remains deferred.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use schema_model::{Cardinality, FieldRef, ObjectTypeRef, ScalarType};

/// Resolved select query.
///
/// The root object type, output shape, filter, and order expressions have all
/// been checked against the schema catalog before this value is constructed.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    root_object_type: ObjectTypeRef,
    shape: ResolvedShape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl SelectQuery {
    pub fn new(
        root_object_type: ObjectTypeRef,
        shape: ResolvedShape,
        filter: Option<Expr>,
        order_by: Vec<OrderExpr>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Self {
        Self {
            root_object_type,
            shape,
            filter,
            order_by,
            limit,
            offset,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn limit(&self) -> Option<i64> {
        self.limit
    }

    pub fn offset(&self) -> Option<i64> {
        self.offset
    }

    pub fn order_by(&self) -> &[OrderExpr] {
        &self.order_by
    }

    pub fn shape(&self) -> &ResolvedShape {
        &self.shape
    }
}

/// Resolved insert query.
///
/// The target object and every assigned field are stable schema references.
/// Assignments retain source order and contain values already checked against
/// field type, nullability, and link-cardinality rules by the resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertQuery {
    root_object_type: ObjectTypeRef,
    assignments: Vec<Assignment>,
}

impl InsertQuery {
    pub fn new(root_object_type: ObjectTypeRef, assignments: Vec<Assignment>) -> Self {
        Self {
            root_object_type,
            assignments,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn assignments(&self) -> &[Assignment] {
        &self.assignments
    }
}

/// Resolved update query.
///
/// The target object, optional filter, and every assignment use stable schema
/// references and values already checked by the resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateQuery {
    target_object_type: ObjectTypeRef,
    filter: Option<Expr>,
    assignments: Vec<Assignment>,
}

impl UpdateQuery {
    pub fn new(
        target_object_type: ObjectTypeRef,
        filter: Option<Expr>,
        assignments: Vec<Assignment>,
    ) -> Self {
        Self {
            target_object_type,
            filter,
            assignments,
        }
    }

    pub fn target_object_type(&self) -> &ObjectTypeRef {
        &self.target_object_type
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn assignments(&self) -> &[Assignment] {
        &self.assignments
    }
}

/// Resolved result shape for one object source.
///
/// Fields are ordered in the same order requested by the query. Nested shapes
/// preserve link cardinality so runtime shaping can distinguish optional,
/// required, and multi-valued relations.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShape {
    source_object_type: ObjectTypeRef,
    items: Vec<ResolvedShapeItem>,
}

impl ResolvedShape {
    pub fn new(source_object_type: ObjectTypeRef, fields: Vec<ResolvedShapeField>) -> Self {
        let items = fields.into_iter().map(ResolvedShapeItem::Field).collect();
        Self {
            source_object_type,
            items,
        }
    }

    pub fn with_items(source_object_type: ObjectTypeRef, items: Vec<ResolvedShapeItem>) -> Self {
        Self {
            source_object_type,
            items,
        }
    }

    pub fn source_object_type(&self) -> &ObjectTypeRef {
        &self.source_object_type
    }

    pub fn items(&self) -> &[ResolvedShapeItem] {
        &self.items
    }

    pub fn fields(&self) -> Vec<&ResolvedShapeField> {
        self.items
            .iter()
            .filter_map(ResolvedShapeItem::as_field)
            .collect()
    }
}

/// One resolved output item in a [`ResolvedShape`].
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedShapeItem {
    Field(ResolvedShapeField),
    Computed(ResolvedComputedField),
}

impl ResolvedShapeItem {
    pub fn output_name(&self) -> &str {
        match self {
            Self::Field(field) => field.output_name(),
            Self::Computed(computed) => computed.output_name(),
        }
    }

    pub fn cardinality(&self) -> Cardinality {
        match self {
            Self::Field(field) => field.cardinality(),
            Self::Computed(computed) => computed.cardinality(),
        }
    }

    pub fn as_field(&self) -> Option<&ResolvedShapeField> {
        match self {
            Self::Field(field) => Some(field),
            Self::Computed(_) => None,
        }
    }

    pub fn as_computed(&self) -> Option<&ResolvedComputedField> {
        match self {
            Self::Field(_) => None,
            Self::Computed(computed) => Some(computed),
        }
    }
}

/// One resolved output field in a [`ResolvedShape`].
///
/// Scalar fields have no child shape. Link fields selected in output must have
/// a child shape, because relations are returned as nested objects rather than
/// as raw foreign key columns.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShapeField {
    output_name: String,
    field: FieldRef,
    cardinality: Cardinality,
    child_shape: Option<ResolvedShape>,
}

/// Query-local computed output item in a [`ResolvedShape`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedComputedField {
    output_name: String,
    value: ValueExpr,
    scalar_type: ScalarType,
    cardinality: Cardinality,
}

impl ResolvedComputedField {
    pub fn new(
        output_name: impl Into<String>,
        value: ValueExpr,
        scalar_type: ScalarType,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            output_name: output_name.into(),
            value,
            scalar_type,
            cardinality,
        }
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn value(&self) -> &ValueExpr {
        &self.value
    }

    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

impl ResolvedShapeField {
    pub fn new(
        output_name: impl Into<String>,
        field: FieldRef,
        cardinality: Cardinality,
        child_shape: Option<ResolvedShape>,
    ) -> Self {
        Self {
            output_name: output_name.into(),
            field,
            cardinality,
            child_shape,
        }
    }
    pub fn output_name(&self) -> &str {
        &self.output_name
    }
    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
    pub fn child_shape(&self) -> Option<&ResolvedShape> {
        self.child_shape.as_ref()
    }
    pub fn field(&self) -> &FieldRef {
        &self.field
    }
}

/// Resolved field path from a root object type.
///
/// The path stores each schema field reference and the combined result
/// cardinality. A path through an optional link is optional; a path through a
/// multi link is many.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    root_object_type: ObjectTypeRef,
    steps: Vec<ResolvedPathStep>,
    result_cardinality: Cardinality,
}

impl ResolvedPath {
    pub fn try_new(
        root_object_type: ObjectTypeRef,
        steps: Vec<ResolvedPathStep>,
    ) -> Result<Self, ResolvedPathError> {
        if steps.is_empty() {
            return Err(ResolvedPathError::EmptyPath);
        }

        Ok(Self::new(root_object_type, steps))
    }

    fn new(root_object_type: ObjectTypeRef, steps: Vec<ResolvedPathStep>) -> Self {
        fn combine_cardinality(left: Cardinality, right: Cardinality) -> Cardinality {
            match (left, right) {
                (Cardinality::Many, _) | (_, Cardinality::Many) => Cardinality::Many,
                (Cardinality::Optional, _) | (_, Cardinality::Optional) => Cardinality::Optional,
                (Cardinality::Required, Cardinality::Required) => Cardinality::Required,
            }
        }

        let result_cardinality = steps
            .iter()
            .map(|step| step.cardinality())
            .fold(Cardinality::Required, combine_cardinality);

        Self {
            root_object_type,
            steps,
            result_cardinality,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn steps(&self) -> &[ResolvedPathStep] {
        &self.steps
    }

    pub fn result_cardinality(&self) -> Cardinality {
        self.result_cardinality
    }
}

/// Errors that can occur while constructing a resolved path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPathError {
    EmptyPath,
}

/// One resolved step in a [`ResolvedPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPathStep {
    field: FieldRef,
    kind: ResolvedPathStepKind,
    cardinality: Cardinality,
}

impl ResolvedPathStep {
    pub fn scalar(field: FieldRef, cardinality: Cardinality) -> Self {
        Self {
            field,
            kind: ResolvedPathStepKind::Scalar,
            cardinality,
        }
    }

    pub fn link(
        field: FieldRef,
        target_object_type: ObjectTypeRef,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            field,
            kind: ResolvedPathStepKind::Link { target_object_type },
            cardinality,
        }
    }

    pub fn field(&self) -> &FieldRef {
        &self.field
    }

    pub fn kind(&self) -> &ResolvedPathStepKind {
        &self.kind
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

/// Semantic kind of a resolved path step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPathStepKind {
    Scalar,
    Link { target_object_type: ObjectTypeRef },
}

/// Resolved boolean expression.
///
/// `IsNull` and `IsNotNull` are used when the source query compares a path to
/// `null`. Keeping them as separate nodes lets SQL generation render
/// `IS NULL` and `IS NOT NULL` instead of binding `null` with a comparison
/// operator.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Compare(CompareExpr),
    IsNull(ValueExpr),
    IsNotNull(ValueExpr),
    In(InExpr),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

/// Resolved comparison expression.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareExpr {
    left: ValueExpr,
    op: CompareOp,
    right: ValueExpr,
}

impl CompareExpr {
    pub fn new(left: ValueExpr, op: CompareOp, right: ValueExpr) -> Self {
        Self { left, op, right }
    }

    pub fn left(&self) -> &ValueExpr {
        &self.left
    }

    pub fn op(&self) -> CompareOp {
        self.op
    }

    pub fn right(&self) -> &ValueExpr {
        &self.right
    }
}

/// Comparison operators implemented by the current IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Resolved membership expression.
#[derive(Debug, Clone, PartialEq)]
pub struct InExpr {
    left: ValueExpr,
    op: InOp,
    right: Vec<ValueExpr>,
}

impl InExpr {
    pub fn new(left: ValueExpr, op: InOp, right: Vec<ValueExpr>) -> Self {
        Self { left, op, right }
    }

    pub fn left(&self) -> &ValueExpr {
        &self.left
    }

    pub fn op(&self) -> InOp {
        self.op
    }

    pub fn right(&self) -> &[ValueExpr] {
        &self.right
    }
}

/// Membership operators implemented by the current IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InOp {
    In,
    NotIn,
}

/// Resolved ordering expression.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderExpr {
    value: ValueExpr,
    direction: OrderDirection,
}

impl OrderExpr {
    pub fn new(value: ValueExpr, direction: OrderDirection) -> Self {
        Self { value, direction }
    }

    pub fn value(&self) -> &ValueExpr {
        &self.value
    }

    pub fn direction(&self) -> OrderDirection {
        self.direction
    }
}

/// Sort direction for a resolved order expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// Scalar value expression used in filters and ordering.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    Path(ResolvedPath),
    Literal(Literal),
    Arithmetic(ArithmeticExpr),
    UnaryArithmetic(UnaryArithmeticExpr),
    Cast(CastExpr),
    StringFunction(StringFunctionExpr),
}

/// Resolved built-in string value function.
#[derive(Debug, Clone, PartialEq)]
pub struct StringFunctionExpr {
    kind: StringFunctionKind,
    args: Vec<StringFunctionArg>,
    cardinality: Cardinality,
}

impl StringFunctionExpr {
    pub fn new(
        kind: StringFunctionKind,
        args: Vec<StringFunctionArg>,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            kind,
            args,
            cardinality,
        }
    }

    pub fn kind(&self) -> StringFunctionKind {
        self.kind
    }

    pub fn args(&self) -> &[StringFunctionArg] {
        &self.args
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

/// Built-in string value functions accepted by Semantic IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFunctionKind {
    Concat,
    Str,
}

/// One resolved string function argument.
#[derive(Debug, Clone, PartialEq)]
pub struct StringFunctionArg {
    value: ValueExpr,
    scalar_type: ScalarType,
}

impl StringFunctionArg {
    pub fn new(value: ValueExpr, scalar_type: ScalarType) -> Self {
        Self { value, scalar_type }
    }

    pub fn value(&self) -> &ValueExpr {
        &self.value
    }

    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }
}

/// Resolved explicit scalar cast value expression.
#[derive(Debug, Clone, PartialEq)]
pub struct CastExpr {
    operand: Box<ValueExpr>,
    target_type: ScalarType,
}

impl CastExpr {
    pub fn new(operand: ValueExpr, target_type: ScalarType) -> Self {
        Self {
            operand: Box::new(operand),
            target_type,
        }
    }

    pub fn operand(&self) -> &ValueExpr {
        &self.operand
    }

    pub fn target_type(&self) -> ScalarType {
        self.target_type
    }
}

/// Resolved arithmetic value expression.
///
/// Arithmetic expressions are scalar value expressions, not boolean filter
/// expressions. The resolver stores the result scalar type after checking both
/// operands so later planning stages do not need to repeat type inference.
#[derive(Debug, Clone, PartialEq)]
pub struct ArithmeticExpr {
    left: Box<ValueExpr>,
    op: ArithmeticOp,
    right: Box<ValueExpr>,
    scalar_type: ScalarType,
}

impl ArithmeticExpr {
    pub fn new(
        left: ValueExpr,
        op: ArithmeticOp,
        right: ValueExpr,
        scalar_type: ScalarType,
    ) -> Self {
        Self {
            left: Box::new(left),
            op,
            right: Box::new(right),
            scalar_type,
        }
    }

    pub fn left(&self) -> &ValueExpr {
        &self.left
    }

    pub fn op(&self) -> ArithmeticOp {
        self.op
    }

    pub fn right(&self) -> &ValueExpr {
        &self.right
    }

    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }
}

/// Arithmetic operators implemented by the current IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Resolved unary arithmetic value expression.
#[derive(Debug, Clone, PartialEq)]
pub struct UnaryArithmeticExpr {
    op: UnaryArithmeticOp,
    operand: Box<ValueExpr>,
    scalar_type: ScalarType,
}

impl UnaryArithmeticExpr {
    pub fn new(op: UnaryArithmeticOp, operand: ValueExpr, scalar_type: ScalarType) -> Self {
        Self {
            op,
            operand: Box::new(operand),
            scalar_type,
        }
    }

    pub fn op(&self) -> UnaryArithmeticOp {
        self.op
    }

    pub fn operand(&self) -> &ValueExpr {
        &self.operand
    }

    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }
}

/// Unary arithmetic operators implemented by the current IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryArithmeticOp {
    Plus,
    Minus,
}

/// Literal values represented by the current IR.
///
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

/// One resolved field assignment in an insert query.
///
/// `field` identifies the destination schema field. `value` distinguishes
/// stored scalar expressions from single-link object identifiers and explicit
/// nulls so backend planners do not need to infer mutation semantics again.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    field: FieldRef,
    value: AssignmentValue,
}

impl Assignment {
    pub fn new(field: FieldRef, value: AssignmentValue) -> Self {
        Self { field, value }
    }

    pub fn field(&self) -> &FieldRef {
        &self.field
    }

    pub fn value(&self) -> &AssignmentValue {
        &self.value
    }
}

/// Value forms supported by the literal-only insert IR.
///
/// A link identifier is kept distinct from a scalar string. Scalar and link
/// nulls remain distinct so backend planners can choose the correct physical
/// column without consulting the schema catalog again.
#[derive(Debug, Clone, PartialEq)]
pub enum AssignmentValue {
    Scalar(Literal),
    LinkId(String),
    ScalarNull,
    LinkNull,
}

#[cfg(test)]
mod tests;
