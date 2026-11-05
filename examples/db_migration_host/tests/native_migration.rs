#[test]
#[ignore = "builds three SDK DLLs and exercises SQLite on disk"]
fn native_database_and_host_resource_migration() {
    db_migration_host::demo::verify().unwrap();
}
