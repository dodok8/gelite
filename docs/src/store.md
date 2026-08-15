# Store

This Northwind-style example models a purchase order as a header plus explicit
line items. `OrderItem` represents the many-to-many relationship between
purchase orders and products while keeping mutations executable with the
current single-link syntax.

```text
{{#include ../../examples/store.geli}}
```

## Create data

```sh
cargo run -p gelite-cli -- schema apply examples/store.geli --database store.db
cargo run -p gelite-cli -- repl --database store.db
```

Insert a customer and two products:

```text
insert Customer {
  email := "margo@example.com",
  name := "Margo Hosho",
  tier := "gold"
}
```

```text
insert Product {
  sku := "CASE-NOTEBOOK",
  name := "Detective Notebook",
  price := 12.5,
  active := true
}
```

```text
insert Product {
  sku := "OWL-PIN",
  name := "Owl Enamel Pin",
  price := 8.0,
  active := true
}
```

Create the purchase order and its items in one interactive transaction. Each
link assignment finds the related object through a unique business key:

```text
start transaction
```

```text
insert PurchaseOrder {
  order_no := "TRIAL-0001",
  status := "paid",
  ordered_at := "2026-08-03T10:00:00Z",
  customer := (
    select Customer { id }
    filter .email = "margo@example.com"
  )
}
```

```text
insert OrderItem {
  quantity := 2,
  unit_price := 12.5,
  purchase := (
    select PurchaseOrder { id }
    filter .order_no = "TRIAL-0001"
  ),
  product := (
    select Product { id }
    filter .sku = "CASE-NOTEBOOK"
  )
}
```

```text
insert OrderItem {
  quantity := 1,
  unit_price := 8.0,
  purchase := (
    select PurchaseOrder { id }
    filter .order_no = "TRIAL-0001"
  ),
  product := (
    select Product { id }
    filter .sku = "OWL-PIN"
  )
}
```

```text
commit
```

## Query line totals

```text
select OrderItem {
  line_total := f64(.quantity) * .unit_price,
  product: {
    sku,
    name
  },
  purchase: {
    order_no,
    status,
    customer: {
      name,
      tier
    }
  }
}
filter .purchase.status in ["paid", "shipped"]
  and .product.active = true
order by f64(.quantity) * .unit_price desc
```

Computed projections are executed by SQLite. Their current REPL column labels
are generated implementation aliases rather than the logical output names.
