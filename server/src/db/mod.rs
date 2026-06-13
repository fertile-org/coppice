mod pool;

#[cfg(feature = "embedded-test-db")]
mod test_embed;

pub use pool::{
    connect_and_migrate, connect_and_migrate_for_tests, shared_test_pool, truncate_test_workspace,
};

#[cfg(feature = "embedded-test-db")]
pub use test_embed::{embedded_test_pool, session_database_url, use_external_test_db};
