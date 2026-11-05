#![cfg(all(not(miri), feature = "dev-tools"))]
#[allow(dead_code)]
#[path = "../examples/threaded_io.rs"]
mod demo;

#[test]
#[ignore = "builds native SDK DLLs with Cargo and uses TCP"]
fn multiple_logic_owners_reload_dlls_while_host_owns_io() {
    demo::verify().unwrap();
}
