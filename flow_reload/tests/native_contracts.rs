#![cfg(all(not(miri), feature = "dev-tools"))]
#[allow(dead_code)]
#[path = "../examples/contracts.rs"]
mod demo;
#[test]
#[ignore = "builds two SDK-native DLL generations with Cargo"]
fn native_service_contracts_and_typed_initialization() {
    demo::verify().unwrap();
}
