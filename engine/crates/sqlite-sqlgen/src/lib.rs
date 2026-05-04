use sqlite_plan::SQLiteSelectPlan;

pub fn render_select(plan: &sqlite_plan::SQLiteSelectPlan) -> SQLiteSelectStatement {
    let select_clause = render_select_clause(plan);
    let from_clause = render_from_clause(plan);

    SQLiteSelectStatement {
        sql: format!("{select_clause} {from_clause}"),
    }
}

fn render_select_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan
        .selected_values()
        .iter()
        .map(|value| format!("{}.{}", value.source_alias(), value.column_name()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("SELECT {columns}")
}

fn render_from_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan.root_source().table_name();
    let alias = plan.root_source().alias();

    format!("FROM {columns} AS {alias}")
}

pub struct SQLiteSelectStatement {
    sql: String,
}

impl SQLiteSelectStatement {
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

#[cfg(test)]
mod tests;
