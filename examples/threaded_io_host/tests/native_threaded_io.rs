
#[test]
#[ignore = "builds native SDK DLLs with Cargo and uses TCP"]
fn multiple_logic_owners_reload_dlls_while_host_owns_io() {
    threaded_io_host::verify().unwrap();
}
