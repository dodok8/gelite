use ir::SelectQuery;
use schema::ObjectTypeRef;

pub fn plan_select(ir: &SelectQuery) -> SQLiteSelectPlan {
    let root_object_type = ir.root_object_type().clone();

    let selected_values = ir
        .shape()
        .fields()
        .iter()
        .map(|field| SQLiteSelectValue::root_scalar(field.field().clone(), field.output_name()))
        .collect();

    SQLiteSelectPlan {
        root_source: SQLiteObjectSource {
            table_name: root_object_type.name().to_ascii_lowercase().to_string(),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: root_object_type,
        },
        selected_values,
        limit: ir.limit(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteValueRole {
    RootScalar,
}

pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    limit: Option<u64>,
}

impl SQLiteSelectPlan {
    pub fn root_source(&self) -> &SQLiteObjectSource {
        &self.root_source
    }

    pub fn selected_values(&self) -> &[SQLiteSelectValue] {
        &self.selected_values
    }

    pub fn limit(&self) -> Option<u64> {
        self.limit
    }
}

pub struct SQLiteSelectValue {
    source_alias: String,
    column_name: String,
    output_name: String,
    field: schema::FieldRef,
    role: SQLiteValueRole,
}

impl SQLiteSelectValue {
    pub fn root_scalar(field: schema::FieldRef, output_name: impl Into<String>) -> Self {
        Self {
            source_alias: "root".to_string(),
            column_name: field.name().to_string(),
            output_name: output_name.into(),
            field,
            role: SQLiteValueRole::RootScalar,
        }
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn field(&self) -> &schema::FieldRef {
        &self.field
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

pub struct SQLiteObjectSource {
    object_type: ObjectTypeRef,
    table_name: String,
    alias: String,
    id_column: String,
}

impl SQLiteObjectSource {
    pub fn object_type(&self) -> &ObjectTypeRef {
        &self.object_type
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn id_column(&self) -> &str {
        &self.id_column
    }
}

#[cfg(test)]
mod tests;
