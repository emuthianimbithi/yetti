# Incremental synchronization

Yetii does not generate or modify incremental SQL. The query author defines the cursor predicate, ordering, and database-specific row limit. Yetii binds saved cursor values, delivers each page, and checkpoints the result.

## Composite cursors

Use `max_tuple` when one field does not uniquely order rows. Tuple cursors support any length of two or more components:

```yaml
query:
  sql: |
    SELECT tenant_id, updated_at, id, status
    FROM orders
    WHERE tenant_id > $last_tenant
       OR (tenant_id = $last_tenant AND updated_at > $last_updated)
       OR (tenant_id = $last_tenant AND updated_at = $last_updated AND id > $last_id)
    ORDER BY tenant_id, updated_at, id
    LIMIT 1000
  parameters:
    last_tenant:
      type: bigint
      source: state_file
      default: "0"
    last_updated:
      type: timestamp
      source: state_file
      default: "1970-01-01T00:00:00Z"
    last_id:
      type: bigint
      source: state_file
      default: "0"

watermark:
  strategy: max_tuple
  columns: [tenant_id, updated_at, id]
  parameters: [last_tenant, last_updated, last_id]
  page_size: 1000
```

The positions in `columns` and `parameters` are paired. Both lists must have the same length and cannot contain duplicates. Every parameter must use `source: state_file`, and every returned cursor value must be present and non-null.

The last component should normally be unique and immutable. For `(updated_at, id)`, multiple rows can share `updated_at`, while `id` provides deterministic progress through that timestamp.

Create an index matching the cursor order:

```sql
CREATE INDEX orders_sync_cursor_idx ON orders (tenant_id, updated_at, id);
```

## Page execution

When `page_size` is configured, Yetii:

1. Loads and binds the current tuple.
2. Executes the query on a persistent ODBC connection.
3. Rejects a result larger than `page_size`.
4. Delivers the page in HTTP batches.
5. Atomically saves the maximum returned tuple after every batch succeeds.
6. Executes the query again when exactly `page_size` rows were returned.
7. Stops on an empty or short page.

The SQL row limit must equal `watermark.page_size`. If it is smaller, Yetii can stop before the source is exhausted. If it is larger or missing, Yetii rejects oversized results instead of silently using unbounded memory.

Yetii fails the run if a non-empty page does not advance the tuple. This catches incorrect `WHERE` clauses such as an inclusive boundary that repeatedly returns the final row.

State advances per delivered page. If a later page fails, the next run resumes after the last successful page. HTTP delivery remains at-least-once: a failed HTTP batch can cause rows from that page to be resent, so receiving endpoints should use upserts or idempotency.

## Database-specific limits

The cursor predicate and `ORDER BY` remain the same. Use the row-limiting syntax supported by the source database.

PostgreSQL and MySQL:

```sql
SELECT id, updated_at, status
FROM orders
WHERE updated_at > $last_updated_at
   OR (updated_at = $last_updated_at AND id > $last_id)
ORDER BY updated_at, id
LIMIT 1000
```

SQL Server:

```sql
SELECT TOP (1000) id, updated_at, status
FROM orders
WHERE updated_at > $last_updated_at
   OR (updated_at = $last_updated_at AND id > $last_id)
ORDER BY updated_at, id
```

Oracle 12c and newer:

```sql
SELECT id, updated_at, status
FROM orders
WHERE updated_at > $last_updated_at
   OR (updated_at = $last_updated_at AND id > $last_id)
ORDER BY updated_at, id
FETCH FIRST 1000 ROWS ONLY
```

## Correctness requirements

- Cursor ordering in `WHERE`, `ORDER BY`, and `watermark.columns` must match.
- Cursor fields must be returned by the query.
- Cursor fields must not become smaller after a row has been processed.
- Rows inserted later with cursor values behind the saved tuple cannot be detected.
- All tuple components are written together only after successful page delivery.
- Concurrent jobs in one Yetii process cannot move a tuple backwards.
- Multiple Yetii processes must not share one state file.

Use full synchronization (`watermark.strategy: none`) when the source has no reliable ordered cursor. Database CDC or change-tracking is preferable when rows can be deleted or backdated changes must be captured.
