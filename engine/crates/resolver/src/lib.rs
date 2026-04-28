use query_ast::Path;

pub fn resolve_select(
    catalog: &schema::SchemaCatalog,
    query: &query_ast::SelectQuery,
) -> Result<ir::SelectQuery, ResolveError> {
    let root_object_type = catalog
        .find_type_ref(query.root_type_name())
        .ok_or_else(|| ResolveError::UnknownObjectType {
            name: query.root_type_name().to_string(),
        })?;

    let fields = query
        .shape()
        .items()
        .iter()
        .map(|item| resolve_shape_item(catalog, &root_object_type, item))
        .collect::<Result<Vec<_>, ResolveError>>()?;

    let filter = query
        .filter()
        .map(|expr| resolve_expr(catalog, &root_object_type, expr))
        .transpose()?;

    let order_by = query
        .order_by()
        .iter()
        .map(|order| resolve_order_expr(catalog, &root_object_type, order))
        .collect::<Result<Vec<_>, ResolveError>>()?;

    Ok(ir::SelectQuery::new(
        root_object_type.clone(),
        ir::ResolvedShape::new(root_object_type, fields),
        filter,
        order_by,
        None,
        None,
    ))
}

fn resolve_shape_item(
    catalog: &schema::SchemaCatalog,
    source_object_type: &schema::ObjectTypeRef,
    item: &query_ast::ShapeItem,
) -> Result<ir::ResolvedShapeField, ResolveError> {
    let steps = item.path().steps();

    if steps.len() != 1 {
        return Err(ResolveError::UnsupportedPath);
    }

    let field_name = steps[0].field_name();

    let field = catalog
        .find_field(source_object_type.name(), field_name)
        .ok_or_else(|| ResolveError::UnknownField {
            object_type: source_object_type.name().to_string(),
            field: field_name.to_string(),
        })?;

    let field_ref = catalog
        .find_field_ref(source_object_type.name(), field_name)
        .expect("field ref should exist for a field already found in the catalog");

    match field {
        schema::Field::Scalar(_) => {
            if item.child_shape().is_some() {
                return Err(ResolveError::NestedShapeOnScalarField {
                    object_type: source_object_type.name().to_string(),
                    field: field_name.to_string(),
                });
            }

            Ok(ir::ResolvedShapeField::new(
                field_name,
                field_ref,
                field.cardinality(),
                None,
            ))
        }
        schema::Field::Link(link) => {
            let child_shape =
                item.child_shape()
                    .ok_or_else(|| ResolveError::MissingShapeOnLinkField {
                        object_type: source_object_type.name().to_string(),
                        field: field_name.to_string(),
                    })?;

            let target_object_type =
                catalog
                    .find_type_ref(link.target_type_name())
                    .ok_or_else(|| ResolveError::UnknownObjectType {
                        name: link.target_type_name().to_string(),
                    })?;

            let child_fields = child_shape
                .items()
                .iter()
                .map(|child_item| resolve_shape_item(catalog, &target_object_type, child_item))
                .collect::<Result<Vec<_>, ResolveError>>()?;

            let resolved_child_shape = ir::ResolvedShape::new(target_object_type, child_fields);

            Ok(ir::ResolvedShapeField::new(
                field_name,
                field_ref,
                field.cardinality(),
                Some(resolved_child_shape),
            ))
        }
    }
}

fn resolve_expr(
    catalog: &schema::SchemaCatalog,
    source_object_type: &schema::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<ir::Expr, ResolveError> {
    match expr {
        query_ast::Expr::Compare(compare) => {
            let left = resolve_path_expr(catalog, source_object_type, compare.left())?;
            let right = resolve_literal_expr(compare.right())?;

            Ok(ir::Expr::Compare(ir::CompareExpr::new(
                left,
                ir::CompareOp::Eq,
                right,
            )))
        }
        query_ast::Expr::Literal(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "literal".to_string(),
        }),
        query_ast::Expr::Path(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "path".to_string(),
        }),
    }
}

fn resolve_path_expr(
    catalog: &schema::SchemaCatalog,
    source_object_type: &schema::ObjectTypeRef,
    path: &Path,
) -> Result<ir::ValueExpr, ResolveError> {
    let steps = path.steps();

    if steps.len() != 1 {
        return Err(ResolveError::UnsupportedPath);
    }

    let field_name = steps[0].field_name();

    let field = catalog
        .find_field(source_object_type.name(), field_name)
        .ok_or_else(|| ResolveError::UnknownField {
            object_type: source_object_type.name().to_string(),
            field: field_name.to_string(),
        })?;

    if !field.is_scalar() {
        return Err(ResolveError::UnsupportedPath);
    }

    let field_ref = catalog
        .find_field_ref(source_object_type.name(), field_name)
        .expect("field ref should exist for a field already found in the catalog");

    Ok(ir::ValueExpr::Field(field_ref))
}

fn resolve_literal_expr(literal: &query_ast::Literal) -> Result<ir::ValueExpr, ResolveError> {
    match literal {
        query_ast::Literal::String(value) => {
            Ok(ir::ValueExpr::Literal(ir::Literal::String(value.clone())))
        }
        _ => Err(ResolveError::UnsupportedLiteral {
            literal: format!("{literal:?}"),
        }),
    }
}

fn resolve_order_expr(
    catalog: &schema::SchemaCatalog,
    source_object_type: &schema::ObjectTypeRef,
    order: &query_ast::OrderExpr,
) -> Result<ir::OrderExpr, ResolveError> {
    let value = resolve_path_expr(catalog, source_object_type, order.path())?;
    let direction = match order.direction() {
        query_ast::OrderDirection::Asc => ir::OrderDirection::Asc,
        query_ast::OrderDirection::Desc => ir::OrderDirection::Desc,
    };

    Ok(ir::OrderExpr::new(value, direction))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownObjectType { name: String },
    UnknownField { object_type: String, field: String },
    NestedShapeOnScalarField { object_type: String, field: String },
    MissingShapeOnLinkField { object_type: String, field: String },
    UnsupportedPath,
    UnsupportedExpr { expr_type: String },
    UnsupportedLiteral { literal: String },
}

#[cfg(test)]
mod tests;
