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

    Ok(ir::SelectQuery::new(
        root_object_type.clone(),
        ir::ResolvedShape::new(root_object_type, fields),
        None,
        vec![],
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownObjectType { name: String },
    UnknownField { object_type: String, field: String },
    NestedShapeOnScalarField { object_type: String, field: String },
    MissingShapeOnLinkField { object_type: String, field: String },
    UnsupportedPath,
}

#[cfg(test)]
mod tests;
