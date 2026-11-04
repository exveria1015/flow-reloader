#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod engine;

use engine::{Command, Controller, Dashboard};
use tauri::State;

#[tauri::command]
fn get_dashboard(state: State<'_, Controller>) -> Dashboard {
    state.snapshot()
}
#[tauri::command]
fn read_source(state: State<'_, Controller>) -> Result<String, String> {
    state.read_source()
}
#[tauri::command]
fn initial_source(state: State<'_, Controller>) -> String {
    state.initial_source.clone()
}
#[tauri::command]
fn save_source(source: String, state: State<'_, Controller>) -> Result<(), String> {
    state.save_source(source)
}
#[tauri::command]
fn rebuild(state: State<'_, Controller>) -> Result<(), String> {
    state.send(Command::Build)
}
#[tauri::command]
fn set_reload_mode(mode: String, state: State<'_, Controller>) -> Result<(), String> {
    if mode != "drain" && mode != "restart" {
        return Err("Unknown reload mode".into());
    }
    state.send(Command::Mode(mode))
}
#[tauri::command]
fn send_request(text: String, slow: bool, state: State<'_, Controller>) -> Result<(), String> {
    if text.len() > 256 {
        return Err("Message must be no longer than 256 bytes".into());
    }
    state.send(Command::Request { text, slow })
}
#[tauri::command]
fn resume(state: State<'_, Controller>) -> Result<(), String> {
    state.send(Command::Resume)
}

fn main() {
    let controller = Controller::new().expect("sample source must be present");
    let stop = controller.stop.clone();
    let engine = controller.spawn();
    let mut engine = Some(engine);
    tauri::Builder::default()
        .manage(controller)
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            read_source,
            initial_source,
            save_source,
            rebuild,
            set_reload_mode,
            send_request,
            resume
        ])
        .build(tauri::generate_context!())
        .expect("failed to create the sample window")
        .run(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                stop.store(true, std::sync::atomic::Ordering::Release);
                if let Some(engine) = engine.take() {
                    let _ = engine.join();
                }
            }
        });
}
