#[test]
#[ignore = "builds two SDK-native DLL generations with Cargo"]
fn native_service_contracts_and_typed_initialization() {
    contracts_host::verify().unwrap();
}
