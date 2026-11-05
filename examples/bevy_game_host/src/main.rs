use bevy::prelude::*;
use bevy_game_host::{build, game_app, verify, Actor, GameHost};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--verify"] {
        let (slow, fast) = build::variants()?;
        // Both libraries are built from this trusted workspace with the matching SDK.
        return unsafe { verify(&slow, &fast) };
    }
    if !args.is_empty() {
        return Err("usage: bevy_game_host [--verify]".into());
    }
    let path = build::initial()?;
    // Only local trusted workspace logic is loaded. All operations stay on this thread.
    let (mut app, _) = unsafe { game_app(&path) }?;
    app.world_mut().non_send_mut::<GameHost>().builds = Some(build::LiveBuild::new()?);
    app.add_systems(Last, report);
    println!("Headless Bevy game at ~60 frames/s. Edit examples/bevy_game_logic/src/lib.rs (STEP). Ctrl+C exits.");
    println!(
        "Use --verify for bounded native assertions. Build log: {}",
        build::log_path().as_ref().display()
    );
    app.run();
    Ok(())
}

fn report(mut host: NonSendMut<GameHost>, actors: Query<(Entity, &Actor)>) {
    if host.frame.is_multiple_of(60) {
        let position = if let Some((entity, actor)) = actors.iter().next() {
            host.request_job(entity, actor.revision, 30);
            actor.position
        } else {
            0
        };
        println!(
            "frame={} generation={} position={} worker_ticks={} step={} restarting={} error={:?}",
            host.frame,
            host.generation,
            position,
            host.worker_ticks,
            host.last_step,
            host.restarting(),
            host.error
        );
    }
}
