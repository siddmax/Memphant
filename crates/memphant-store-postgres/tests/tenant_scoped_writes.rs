//! Guard against a bug class already fixed twice: an UPDATE/DELETE on a
//! composite-PK memphant table keyed WITHOUT `tenant_id`. Every domain table's
//! PK leads with `tenant_id`, so a tenant-less write cannot use the PK index
//! (the seq-scan class fixed on job_state) and, once the non-owner RLS role
//! lands, would be a cross-tenant hazard. This scans the store source so a new
//! such write fails CI even before it reaches a DB-backed test.
//!
//! No false positives: only genuinely id-only tables are exempt, and the two
//! intentional global job_state maintenance sweeps already carry a `tenant_id`
//! predicate (null-guard / CTE join), so they pass on their own.
//!
//! ponytail: substring scan of store.rs SQL literals, not a SQL parser. Ceiling:
//! a prose comment containing the literal text "update memphant.<t> ... where
//! id" with no tenant_id would false-trip. None exist; upgrade to a real parse
//! only if that ever bites.

const STORE_SRC: &str = include_str!("../src/store.rs");

/// Tables whose primary key is id/version-only (no `tenant_id` column), so a
/// bare-id predicate is correct. Every other memphant table is composite
/// `(tenant_id, ...)`.
const ID_ONLY_TABLES: &[&str] = &["api_key", "tenant", "schema_migrations"];

/// Each `update`/`delete` statement text, sliced from the verb to the end of
/// its double-quoted SQL literal. The store's SQL literals never embed a double
/// quote (SQL string values use single quotes), so the next `"` closes the
/// literal — whether the verb starts the literal or sits mid-literal (as in the
/// claim CTE).
fn write_statements<'a>(src: &'a str, verb: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(pos) = rest.find(verb) {
        let after = &rest[pos..];
        let end = after.find('"').unwrap_or(after.len());
        out.push(&after[..end]);
        rest = &after[end..];
    }
    out
}

fn table_of(statement: &str, verb: &str) -> String {
    statement[verb.len()..]
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("")
        .to_string()
}

#[test]
fn every_write_on_a_composite_pk_table_scopes_by_tenant() {
    let mut checked = 0;
    for verb in ["update memphant.", "delete from memphant."] {
        for statement in write_statements(STORE_SRC, verb) {
            let table = table_of(statement, verb);
            if ID_ONLY_TABLES.contains(&table.as_str()) {
                continue;
            }
            checked += 1;
            assert!(
                statement.contains("tenant_id"),
                "write on composite-PK table `memphant.{table}` is not scoped by \
                 tenant_id: a partial-key predicate can't use the PK index and is a \
                 cross-tenant hazard under RLS. Add `tenant_id` to the WHERE clause, \
                 or add the table to ID_ONLY_TABLES if its PK is genuinely id-only.\n\
                 statement: {statement}"
            );
        }
    }
    assert!(
        checked >= 5,
        "expected to scan several tenant-scoped writes; found {checked} — the scanner \
         is likely broken (guards against a silent pass)"
    );
}
