#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    root_type_name: String,
    shape: Shape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    items: Vec<ShapeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeItem {
    path: Path,
    child_shape: Option<Shape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    steps: Vec<PathStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    field_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Path(Path),
    Compare(CompareExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompareExpr {
    left: Path,
    op: CompareOp,
    right: Literal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderExpr {
    path: Path,
    direction: OrderDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl SelectQuery {
    pub fn new(
        root_type_name: impl Into<String>,
        shape: Shape,
        filter: Option<Expr>,
        order_by: Vec<OrderExpr>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Self {
        Self {
            root_type_name: root_type_name.into(),
            shape,
            filter,
            order_by,
            limit,
            offset,
        }
    }

    pub fn root_type_name(&self) -> &str {
        &self.root_type_name
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn order_by(&self) -> &[OrderExpr] {
        &self.order_by
    }

    pub fn limit(&self) -> Option<u64> {
        self.limit
    }

    pub fn offset(&self) -> Option<u64> {
        self.offset
    }
}

impl Shape {
    pub fn new(items: Vec<ShapeItem>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[ShapeItem] {
        &self.items
    }
}

impl ShapeItem {
    pub fn new(path: Path, child_shape: Option<Shape>) -> Self {
        Self { path, child_shape }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn child_shape(&self) -> Option<&Shape> {
        self.child_shape.as_ref()
    }
}

impl Path {
    pub fn new(steps: Vec<PathStep>) -> Self {
        Path { steps }
    }

    pub fn steps(&self) -> &[PathStep] {
        self.steps.as_ref()
    }
}

impl PathStep {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
        }
    }

    pub fn field_name(&self) -> &str {
        &self.field_name
    }
}

impl OrderExpr {
    pub fn new(path: Path, direction: OrderDirection) -> Self {
        Self { path, direction }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn direction(&self) -> OrderDirection {
        self.direction
    }
}

impl CompareExpr {
    pub fn new(left: Path, op: CompareOp, right: Literal) -> Self {
        Self { left, op, right }
    }
    pub fn left(&self) -> &Path {
        &self.left
    }
    pub fn op(&self) -> CompareOp {
        self.op
    }
    pub fn right(&self) -> &Literal {
        &self.right
    }
}

#[cfg(test)]
mod tests;
