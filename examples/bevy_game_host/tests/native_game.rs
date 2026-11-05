#[test]
#[ignore = "builds two native DLLs with Cargo; run explicitly with --ignored --test-threads=1"]
fn bevy_frames_with_native_reload() {
    let (slow, fast) = bevy_game_host::build::variants().unwrap();
    // These native images are built from the trusted workspace against the same SDK.
    unsafe { bevy_game_host::verify(&slow, &fast) }.unwrap();
}
