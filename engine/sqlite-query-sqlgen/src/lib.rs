#![no_std]
//! SQL renderer for SQLite query plans.
//!
//! This crate serializes `sqlite-query-plan` structures into SQL text and bind
//! values. It does not resolve schema names, choose joins, or inspect query AST
//! nodes. Those responsibilities belong to earlier compiler stages.
//!
//! The renderer emits selects, object mutations, and set-based multi-link
//! add/remove statements. Literal values are emitted as bind placeholders
//! instead of being interpolated into SQL strings.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_query_plan::{
    SQLiteArithmeticOp, SQLiteAssignmentValue, SQLiteCastTarget, SQLiteCompareOp, SQLiteDeletePlan,
    SQLiteFollowUpFetchPlan, SQLiteGeneratedIdStrategy, SQLiteInOp, SQLiteInRhs, SQLiteInsertPlan,
    SQLiteJoin, SQLiteJoinKind, SQLiteLiteral, SQLiteMultiLinkMutationOp,
    SQLiteMultiLinkMutationPlan, SQLiteObjectSource, SQLiteOrderDirection, SQLiteResultShapePlan,
    SQLiteSelectPlan, SQLiteSelectValue, SQLiteStringFunctionKind, SQLiteUnaryArithmeticOp,
    SQLiteUpdatePlan, SQLiteValueExpr, SQLiteWhereExpr,
};

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn render_qualified_identifier(source_alias: &str, column_name: &str) -> String {
    format!(
        "{}.{}",
        quote_identifier(source_alias),
        quote_identifier(column_name)
    )
}

/// Renders a structured SQLite select plan into SQL text and bind values.
pub fn render_select(plan: &sqlite_query_plan::SQLiteSelectPlan) -> SQLiteStatement {
    let (select_clause, mut bind_values) = render_select_clause(plan);
    let from_clause = render_from_clause(plan);
    let (where_clause, where_bind_values) = render_where_clause(plan);
    bind_values.extend(where_bind_values);
    let order_clause = render_order_clause(plan, &mut bind_values);
    let limit_clause = render_limit_clause(plan);
    let offset_clause = render_offset_clause(plan);
    let join_clauses = render_join_clauses(plan);

    let mut clauses = vec![select_clause, from_clause];
    clauses.extend(join_clauses);
    if let Some(where_clause) = where_clause {
        clauses.push(where_clause);
    }
    if let Some(order_clause) = order_clause {
        clauses.push(order_clause);
    }
    if let Some(limit_clause) = limit_clause {
        clauses.push(limit_clause);
    }
    if let Some(offset_clause) = offset_clause {
        clauses.push(offset_clause);
    }

    let output_names = plan
        .selected_values()
        .iter()
        .map(|value| value.output_name().map(str::to_string))
        .collect();

    SQLiteStatement {
        sql: clauses.join(" "),
        bind_values,
        output_names,
        result_shape: Some(render_result_shape(plan.result_shape())),
        parent_identity_column_index: None,
    }
}

/// Renders one batched multi-link follow-up statement.
pub fn render_follow_up(plan: &SQLiteFollowUpFetchPlan, parent_ids: &[String]) -> SQLiteStatement {
    let mut bind_values = Vec::new();
    let parent_alias = plan.source_alias();
    let parent_column = render_qualified_identifier(parent_alias, plan.source_column());
    let mut columns = vec![parent_column.clone()];
    columns.extend(render_select_values(
        plan.selected_values(),
        &mut bind_values,
    ));
    let from = match plan.join_table_name() {
        Some(table) => format!(
            "FROM {} INNER JOIN {} AS {} ON {} = {}",
            if table == plan.source_alias() {
                quote_identifier(table)
            } else {
                format!(
                    "{} AS {}",
                    quote_identifier(table),
                    quote_identifier(plan.source_alias())
                )
            },
            quote_identifier(plan.target_source().table_name()),
            quote_identifier(plan.target_source().alias()),
            render_qualified_identifier(plan.source_alias(), plan.target_column()),
            render_qualified_identifier(
                plan.target_source().alias(),
                plan.target_source().id_column()
            ),
        ),
        None => format!(
            "FROM {} AS {}",
            quote_identifier(plan.target_source().table_name()),
            quote_identifier(plan.target_source().alias())
        ),
    };
    let mut clauses = vec![format!("SELECT {}", columns.join(", ")), from];
    clauses.extend(render_joins(plan.joins()));
    clauses.push(format!(
        "WHERE {} IN ({})",
        parent_column,
        vec!["?"; parent_ids.len()].join(", ")
    ));
    bind_values.extend(parent_ids.iter().cloned().map(SQLiteBindValue::String));

    let mut output_names = vec![None];
    output_names.extend(
        plan.selected_values()
            .iter()
            .map(|value| value.output_name().map(str::to_string)),
    );

    SQLiteStatement {
        sql: clauses.join(" "),
        bind_values,
        output_names,
        result_shape: Some(render_result_shape_from_index(plan.result_shape(), 1)),
        parent_identity_column_index: Some(0),
    }
}

/// Renders a structured SQLite insert plan with a runtime-generated object id.
pub fn render_insert(plan: &SQLiteInsertPlan, generated_id: &str) -> SQLiteStatement {
    let mut columns = vec![quote_identifier(plan.root_target().id_column())];
    let mut values = vec!["?".to_string()];
    let mut bind_values = match plan.generated_id_strategy() {
        SQLiteGeneratedIdStrategy::RuntimeUuid => {
            vec![SQLiteBindValue::String(generated_id.to_string())]
        }
    };

    for assignment in plan.assignments() {
        columns.push(quote_identifier(assignment.column_name()));
        values.push(render_assignment_value(
            assignment.value(),
            &mut bind_values,
        ));
    }

    SQLiteStatement::new(
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote_identifier(plan.root_target().table_name()),
            columns.join(", "),
            values.join(", ")
        ),
        bind_values,
    )
}

/// Renders a structured SQLite update plan into SQL text and bind values.
pub fn render_update(plan: &SQLiteUpdatePlan) -> SQLiteStatement {
    if let Some(mutation) = plan.multi_link_mutation() {
        return render_multi_link_mutation(mutation);
    }

    let mut bind_values = Vec::new();
    let assignments = plan
        .assignments()
        .iter()
        .map(|assignment| {
            let value = render_assignment_value(assignment.value(), &mut bind_values);

            format!("{} = {value}", quote_identifier(assignment.column_name()))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let target = plan.target();
    let mut sql = if plan.joins().is_empty() {
        format!(
            "UPDATE {} AS {} SET {assignments}",
            quote_identifier(target.table_name()),
            quote_identifier(target.alias())
        )
    } else {
        format!(
            "UPDATE {} SET {assignments}",
            quote_identifier(target.table_name())
        )
    };

    append_mutation_filter(
        &mut sql,
        target,
        plan.filter(),
        plan.joins(),
        &mut bind_values,
    );

    SQLiteStatement::new(sql, bind_values)
}

fn render_multi_link_mutation(plan: &SQLiteMultiLinkMutationPlan) -> SQLiteStatement {
    let sources = render_select(plan.sources());
    let targets = render_select(plan.targets());
    let mut bind_values = sources.bind_values().to_vec();
    bind_values.extend_from_slice(targets.bind_values());

    let sql = match plan.operation() {
        SQLiteMultiLinkMutationOp::Add => format!(
            "INSERT INTO {} ({}, {}) SELECT {}, {} FROM ({}) AS {} CROSS JOIN ({}) AS {} WHERE true ON CONFLICT ({}, {}) DO NOTHING",
            quote_identifier(plan.join_table_name()),
            quote_identifier(plan.source_column()),
            quote_identifier(plan.target_column()),
            render_qualified_identifier("source", "id"),
            render_qualified_identifier("target", "id"),
            sources.sql(),
            quote_identifier("source"),
            targets.sql(),
            quote_identifier("target"),
            quote_identifier(plan.source_column()),
            quote_identifier(plan.target_column()),
        ),
        SQLiteMultiLinkMutationOp::Remove => format!(
            "DELETE FROM {} WHERE {} IN ({}) AND {} IN ({})",
            quote_identifier(plan.join_table_name()),
            quote_identifier(plan.source_column()),
            sources.sql(),
            quote_identifier(plan.target_column()),
            targets.sql(),
        ),
    };

    SQLiteStatement::new(sql, bind_values)
}

fn render_assignment_value(
    value: &SQLiteAssignmentValue,
    bind_values: &mut Vec<SQLiteBindValue>,
) -> String {
    match value {
        SQLiteAssignmentValue::Literal(literal) => {
            bind_values.push(bind_value_from_literal(literal));
            "?".to_string()
        }
        SQLiteAssignmentValue::Select(plan) => {
            let statement = render_select(plan);
            bind_values.extend_from_slice(statement.bind_values());
            format!("({})", statement.sql())
        }
    }
}

/// Renders a structured SQLite delete plan into SQL text and bind values.
pub fn render_delete(plan: &SQLiteDeletePlan) -> SQLiteStatement {
    let target = plan.target();
    let mut sql = if plan.filter().is_some() && plan.joins().is_empty() {
        format!(
            "DELETE FROM {} AS {}",
            quote_identifier(target.table_name()),
            quote_identifier(target.alias())
        )
    } else {
        format!("DELETE FROM {}", quote_identifier(target.table_name()))
    };
    let mut bind_values = Vec::new();

    append_mutation_filter(
        &mut sql,
        target,
        plan.filter(),
        plan.joins(),
        &mut bind_values,
    );

    SQLiteStatement::new(sql, bind_values)
}

fn append_mutation_filter(
    sql: &mut String,
    target: &SQLiteObjectSource,
    filter: Option<&SQLiteWhereExpr>,
    joins: &[SQLiteJoin],
    bind_values: &mut Vec<SQLiteBindValue>,
) {
    let Some(filter) = filter else {
        return;
    };
    let filter_sql = render_where_expr(filter, bind_values);

    if joins.is_empty() {
        sql.push_str(&format!(" WHERE {filter_sql}"));
    } else {
        let joins = render_joins(joins).join(" ");
        sql.push_str(&format!(
            " WHERE {} IN (SELECT {} FROM {} AS {} {joins} WHERE {filter_sql})",
            quote_identifier(target.id_column()),
            render_qualified_identifier(target.alias(), target.id_column()),
            quote_identifier(target.table_name()),
            quote_identifier(target.alias()),
        ));
    }
}

fn render_select_clause(plan: &SQLiteSelectPlan) -> (String, Vec<SQLiteBindValue>) {
    let mut bind_values = Vec::new();
    let columns = render_select_values(plan.selected_values(), &mut bind_values).join(", ");

    (format!("SELECT {columns}"), bind_values)
}

fn render_select_values(
    selected_values: &[SQLiteSelectValue],
    bind_values: &mut Vec<SQLiteBindValue>,
) -> Vec<String> {
    selected_values
        .iter()
        .map(|value| {
            let value_sql = render_value_expr(value.value(), bind_values);

            if let Some(computed) = value.as_computed() {
                format!("{value_sql} AS {}", quote_identifier(computed.sql_alias()))
            } else {
                value_sql
            }
        })
        .collect()
}

fn render_from_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan.root_source().table_name();
    let alias = plan.root_source().alias();

    format!(
        "FROM {} AS {}",
        quote_identifier(columns),
        quote_identifier(alias)
    )
}

fn render_where_clause(plan: &SQLiteSelectPlan) -> (Option<String>, Vec<SQLiteBindValue>) {
    match plan.filter() {
        None => (None, vec![]),
        Some(expr) => {
            let mut bind_values = Vec::new();
            let expr_sql = render_where_expr(expr, &mut bind_values);

            (Some(format!("WHERE {expr_sql}")), bind_values)
        }
    }
}

fn render_where_expr(expr: &SQLiteWhereExpr, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match expr {
        SQLiteWhereExpr::Exists(exists) => {
            let mut clauses = vec![format!(
                "SELECT 1 FROM {} AS {}",
                quote_identifier(exists.source().table_name()),
                quote_identifier(exists.source().alias())
            )];
            clauses.extend(render_joins(exists.joins()));
            clauses.push(format!(
                "WHERE {}",
                render_where_expr(exists.predicate(), bind_values)
            ));
            format!("EXISTS ({})", clauses.join(" "))
        }
        SQLiteWhereExpr::Compare(compare) => {
            let left_sql = render_value_expr(compare.left(), bind_values);
            let op_sql = render_compare_op(compare.op());
            let right_sql = render_value_expr(compare.right(), bind_values);

            format!("{left_sql} {op_sql} {right_sql}")
        }
        SQLiteWhereExpr::IsNull(value) => {
            let value_sql = render_value_expr(value, bind_values);

            format!("{value_sql} IS NULL")
        }
        SQLiteWhereExpr::IsNotNull(value) => {
            let value_sql = render_value_expr(value, bind_values);

            format!("{value_sql} IS NOT NULL")
        }
        SQLiteWhereExpr::In(in_expr) => {
            let left_sql = render_value_expr(in_expr.left(), bind_values);
            let op_sql = render_in_op(in_expr.op());
            let right_sql = match in_expr.right() {
                SQLiteInRhs::List(values) => values
                    .iter()
                    .map(|value| render_value_expr(value, bind_values))
                    .collect::<Vec<_>>()
                    .join(", "),
                SQLiteInRhs::Select(plan) => {
                    let statement = render_select(plan);
                    bind_values.extend_from_slice(statement.bind_values());
                    statement.sql().to_string()
                }
            };

            format!("{left_sql} {op_sql} ({right_sql})")
        }
        SQLiteWhereExpr::And(left, right) => {
            let left_sql = render_where_expr(left, bind_values);
            let right_sql = render_where_expr(right, bind_values);

            format!("({left_sql} AND {right_sql})")
        }
        SQLiteWhereExpr::Or(left, right) => {
            let left_sql = render_where_expr(left, bind_values);
            let right_sql = render_where_expr(right, bind_values);

            format!("({left_sql} OR {right_sql})")
        }
        SQLiteWhereExpr::Not(inner) => {
            let inner_sql = render_where_expr(inner, bind_values);

            format!("NOT ({inner_sql})")
        }
    }
}

fn render_join_clauses(plan: &SQLiteSelectPlan) -> Vec<String> {
    render_joins(plan.joins())
}

fn render_joins(joins: &[sqlite_query_plan::SQLiteJoin]) -> Vec<String> {
    joins
        .iter()
        .map(|join| {
            let join_kind = match join.kind() {
                SQLiteJoinKind::Inner => "INNER JOIN",
                SQLiteJoinKind::Left => "LEFT JOIN",
            };

            let on = join.on();

            format!(
                "{join_kind} {} AS {} ON {} = {}",
                quote_identifier(join.target_table()),
                quote_identifier(join.target_alias()),
                render_qualified_identifier(on.left_alias(), on.left_column()),
                render_qualified_identifier(on.right_alias(), on.right_column()),
            )
        })
        .collect()
}

fn render_compare_op(op: SQLiteCompareOp) -> &'static str {
    match op {
        SQLiteCompareOp::Eq => "=",
        SQLiteCompareOp::Ne => "!=",
        SQLiteCompareOp::Lt => "<",
        SQLiteCompareOp::Le => "<=",
        SQLiteCompareOp::Gt => ">",
        SQLiteCompareOp::Ge => ">=",
    }
}

fn render_in_op(op: SQLiteInOp) -> &'static str {
    match op {
        SQLiteInOp::In => "IN",
        SQLiteInOp::NotIn => "NOT IN",
    }
}

fn render_arithmetic_op(op: SQLiteArithmeticOp) -> &'static str {
    match op {
        SQLiteArithmeticOp::Add => "+",
        SQLiteArithmeticOp::Sub => "-",
        SQLiteArithmeticOp::Mul => "*",
        SQLiteArithmeticOp::Div => "/",
        SQLiteArithmeticOp::Mod => "%",
    }
}

fn render_unary_arithmetic_op(op: SQLiteUnaryArithmeticOp) -> &'static str {
    match op {
        SQLiteUnaryArithmeticOp::Plus => "+",
        SQLiteUnaryArithmeticOp::Minus => "-",
    }
}

fn render_cast_target(target: SQLiteCastTarget) -> &'static str {
    match target {
        SQLiteCastTarget::Int64 => "INTEGER",
        SQLiteCastTarget::Float64 => "REAL",
    }
}

fn render_value_expr(value: &SQLiteValueExpr, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match value {
        SQLiteValueExpr::Column(column) => {
            render_qualified_identifier(column.source_alias(), column.column_name())
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => {
            render_literal(&SQLiteLiteral::String(value.clone()), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => {
            render_literal(&SQLiteLiteral::Int64(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Float64(value)) => {
            render_literal(&SQLiteLiteral::Float64(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Bool(value)) => {
            render_literal(&SQLiteLiteral::Bool(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Null) => {
            render_literal(&SQLiteLiteral::Null, bind_values)
        }
        SQLiteValueExpr::Arithmetic(arithmetic) => {
            let left = render_value_expr(arithmetic.left(), bind_values);
            let right = render_value_expr(arithmetic.right(), bind_values);
            let op = render_arithmetic_op(arithmetic.op());

            format!("({left} {op} {right})")
        }
        SQLiteValueExpr::UnaryArithmetic(unary) => {
            let op = render_unary_arithmetic_op(unary.op());
            let operand = render_value_expr(unary.operand(), bind_values);

            format!("({op}{operand})")
        }
        SQLiteValueExpr::Cast(cast) => {
            let operand = render_value_expr(cast.operand(), bind_values);
            let target = render_cast_target(cast.target());

            format!("CAST({operand} AS {target})")
        }
        SQLiteValueExpr::StringFunction(function) => match function.kind() {
            SQLiteStringFunctionKind::Concat => {
                let args = function
                    .args()
                    .iter()
                    .map(|arg| render_value_expr(arg.value(), bind_values))
                    .collect::<Vec<_>>()
                    .join(" || ");

                format!("({args})")
            }
            SQLiteStringFunctionKind::Str => {
                let [arg] = function.args() else {
                    unreachable!("SQLite planner receives only resolver-accepted str arity");
                };

                render_str_value_expr(arg.value(), arg.scalar_type(), bind_values)
            }
        },
    }
}

fn render_str_value_expr(
    value: &SQLiteValueExpr,
    scalar_type: schema_model::ScalarType,
    bind_values: &mut Vec<SQLiteBindValue>,
) -> String {
    match scalar_type {
        schema_model::ScalarType::Str
        | schema_model::ScalarType::Uuid
        | schema_model::ScalarType::DateTime => render_value_expr(value, bind_values),
        schema_model::ScalarType::Int64 | schema_model::ScalarType::Float64 => {
            let value_sql = render_value_expr(value, bind_values);

            format!("CAST({value_sql} AS TEXT)")
        }
        schema_model::ScalarType::Bool => {
            let null_check_sql = render_value_expr(value, bind_values);
            let value_sql = render_value_expr(value, bind_values);

            format!(
                "CASE WHEN {null_check_sql} IS NULL THEN NULL WHEN {value_sql} THEN 'true' ELSE 'false' END"
            )
        }
    }
}

fn render_literal(literal: &SQLiteLiteral, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    bind_values.push(bind_value_from_literal(literal));

    "?".to_string()
}

fn bind_value_from_literal(literal: &SQLiteLiteral) -> SQLiteBindValue {
    match literal {
        SQLiteLiteral::String(value) => SQLiteBindValue::String(value.clone()),
        SQLiteLiteral::Int64(value) => SQLiteBindValue::Int64(*value),
        SQLiteLiteral::Float64(value) => SQLiteBindValue::Float64(*value),
        SQLiteLiteral::Bool(value) => SQLiteBindValue::Bool(*value),
        SQLiteLiteral::Null => SQLiteBindValue::Null,
    }
}

fn render_order_clause(
    plan: &SQLiteSelectPlan,
    bind_values: &mut Vec<SQLiteBindValue>,
) -> Option<String> {
    let orders = plan.order_by();

    if orders.is_empty() {
        return None;
    }

    let order_items = orders
        .iter()
        .map(|order| {
            let value = render_value_expr(order.value(), bind_values);
            let dir = match order.direction() {
                SQLiteOrderDirection::Asc => "ASC",
                SQLiteOrderDirection::Desc => "DESC",
            };

            format!("{value} {dir}")
        })
        .collect::<Vec<String>>()
        .join(", ");

    Some(format!("ORDER BY {order_items}"))
}

fn render_limit_clause(plan: &SQLiteSelectPlan) -> Option<String> {
    let limit = plan.limit();

    limit.map(|val| format!("LIMIT {val}"))
}

fn render_offset_clause(plan: &SQLiteSelectPlan) -> Option<String> {
    let offset = plan.offset();

    offset.map(|val| format!("OFFSET {val}"))
}

/// Rendered SQLite statement and its ordered bind values.
pub struct SQLiteStatement {
    sql: String,
    bind_values: Vec<SQLiteBindValue>,
    output_names: Vec<Option<String>>,
    result_shape: Option<SQLiteResultShape>,
    parent_identity_column_index: Option<usize>,
}

impl SQLiteStatement {
    pub fn new(sql: impl Into<String>, bind_values: Vec<SQLiteBindValue>) -> Self {
        Self {
            sql: sql.into(),
            bind_values,
            output_names: vec![],
            result_shape: None,
            parent_identity_column_index: None,
        }
    }

    pub fn with_result_shape(mut self, result_shape: SQLiteResultShape) -> Self {
        self.result_shape = Some(result_shape);
        self
    }

    pub fn with_parent_identity_column_index(mut self, column_index: usize) -> Self {
        self.parent_identity_column_index = Some(column_index);
        self
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn bind_values(&self) -> &[SQLiteBindValue] {
        &self.bind_values
    }

    pub fn output_names(&self) -> &[Option<String>] {
        &self.output_names
    }

    pub fn result_shape(&self) -> Option<&SQLiteResultShape> {
        self.result_shape.as_ref()
    }

    pub fn parent_identity_column_index(&self) -> Option<usize> {
        self.parent_identity_column_index
    }
}

/// Bind value produced while rendering SQL placeholders.
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteBindValue {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

/// Execution-time mapping from logical fields to a decoded physical SQLite row.
///
/// Identity and field column indexes are absolute positions in the complete
/// physical row, including synthesized identity columns. Nested shapes share
/// the same coordinate space. Field column indexes must be unique because
/// shaping moves each selected value out of its physical slot.
#[derive(Debug)]
pub struct SQLiteResultShape {
    identity_column_index: Option<usize>,
    fields: Vec<SQLiteResultField>,
}

/// One logical output field backed by either a physical column or a nested shape.
#[derive(Debug)]
pub struct SQLiteResultField {
    output_name: String,
    column_index: Option<usize>,
    nested_shape: Option<SQLiteResultShape>,
    follow_up_fetch_index: Option<usize>,
}

impl SQLiteResultShape {
    /// Creates a result shape using absolute physical-row column indexes.
    pub fn new(identity_column_index: Option<usize>, fields: Vec<SQLiteResultField>) -> Self {
        Self {
            identity_column_index,
            fields,
        }
    }

    pub fn identity_column_index(&self) -> Option<usize> {
        self.identity_column_index
    }

    pub fn fields(&self) -> &[SQLiteResultField] {
        &self.fields
    }
}

impl SQLiteResultField {
    pub fn value(output_name: impl Into<String>, column_index: usize) -> Self {
        Self {
            output_name: output_name.into(),
            column_index: Some(column_index),
            nested_shape: None,
            follow_up_fetch_index: None,
        }
    }

    pub fn nested(output_name: impl Into<String>, nested_shape: SQLiteResultShape) -> Self {
        Self {
            output_name: output_name.into(),
            column_index: None,
            nested_shape: Some(nested_shape),
            follow_up_fetch_index: None,
        }
    }

    pub fn follow_up(output_name: impl Into<String>, fetch_index: usize) -> Self {
        Self {
            output_name: output_name.into(),
            column_index: None,
            nested_shape: None,
            follow_up_fetch_index: Some(fetch_index),
        }
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn column_index(&self) -> Option<usize> {
        self.column_index
    }

    pub fn nested_shape(&self) -> Option<&SQLiteResultShape> {
        self.nested_shape.as_ref()
    }

    pub fn follow_up_fetch_index(&self) -> Option<usize> {
        self.follow_up_fetch_index
    }
}

fn render_result_shape(plan: &SQLiteResultShapePlan) -> SQLiteResultShape {
    render_result_shape_from_index(plan, 0)
}

fn render_result_shape_from_index(
    plan: &SQLiteResultShapePlan,
    first_column_index: usize,
) -> SQLiteResultShape {
    let mut next_column_index = first_column_index;

    render_result_shape_from(plan, &mut next_column_index)
}

fn render_result_shape_from(
    plan: &SQLiteResultShapePlan,
    next_column_index: &mut usize,
) -> SQLiteResultShape {
    let identity_column_index = plan.identity_value().map(|_| {
        let index = *next_column_index;
        *next_column_index += 1;
        index
    });

    let fields = plan
        .fields()
        .iter()
        .map(|field| {
            match (
                field.value(),
                field.nested_shape(),
                field.follow_up_fetch_index(),
            ) {
                (Some(_), None, None) => {
                    let column_index = *next_column_index;
                    *next_column_index += 1;

                    SQLiteResultField::value(field.output_name(), column_index)
                }
                (None, Some(nested_plan), None) => SQLiteResultField::nested(
                    field.output_name(),
                    render_result_shape_from(nested_plan, next_column_index),
                ),
                (None, None, Some(fetch_index)) => {
                    SQLiteResultField::follow_up(field.output_name(), fetch_index)
                }
                _ => unreachable!("result field must contain either a value or a nested shape"),
            }
        })
        .collect();

    SQLiteResultShape::new(identity_column_index, fields)
}

#[cfg(test)]
mod tests;
