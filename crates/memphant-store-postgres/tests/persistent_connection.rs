use memphant_core::StoreError;
use memphant_store_postgres::PgStore;

const DIRECT_BUT_UNREACHABLE: &str = "postgresql://memphant@127.0.0.1:1/memphant";
const TRANSACTION_POOLER: &str =
    "postgresql://postgres.example:secret@aws-0.pooler.supabase.com:6543/postgres";
const EXPECTED: &str = "persistent Postgres connections cannot use transaction pooler port 6543; use direct or session port 5432";

fn assert_transaction_pooler(error: StoreError) {
    match error {
        StoreError::Backend(message) => assert_eq!(message, EXPECTED),
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn persistent_database_auth_and_provision_urls_reject_transaction_pooler_before_network() {
    assert_transaction_pooler(
        PgStore::connect_worker(TRANSACTION_POOLER)
            .await
            .err()
            .expect("database URL must be rejected"),
    );
    assert_transaction_pooler(
        PgStore::connect_app(DIRECT_BUT_UNREACHABLE, TRANSACTION_POOLER)
            .await
            .err()
            .expect("auth URL must be validated before connecting the database URL"),
    );
    assert_transaction_pooler(
        PgStore::connect_with_capabilities(
            DIRECT_BUT_UNREACHABLE,
            DIRECT_BUT_UNREACHABLE,
            TRANSACTION_POOLER,
        )
        .await
        .err()
        .expect("provision URL must be validated before connecting the database URL"),
    );
}
