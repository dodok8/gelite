mod fixtures;

use crate::render_select;
use fixtures::{post_query_with_shape, post_title_shape_field};

#[test]
fn sqlite_sqlgen_can_render_simple_root_scalar_select() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(statement.sql(), "SELECT root.title FROM post AS root");
}
