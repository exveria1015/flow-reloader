// Compile every public main! form in both debug and release profiles.
#![allow(dead_code)]

macro_rules! host {
    ($name:ident, $($args:tt)*) => {
        mod $name {
            type AppState = flow_reload::AppState;
            type RuntimeHandle = flow_reload::RuntimeHandle;

            mod logic {
                pub fn hot_run(
                    _: &mut flow_reload::AppState,
                    _: &flow_reload::RuntimeHandle,
                ) -> Result<flow_reload::RunStatus, flow_reload::RunnerError> {
                    Ok(flow_reload::RunStatus::Done)
                }
            }

            flow_reload::main!($($args)*);
        }
    };
}

host!(default_state, logic);
host!(default_state_runtime, logic, runtime: RuntimeHandle::std());
host!(state, logic, AppState);
host!(state_runtime, logic, AppState, runtime: RuntimeHandle::std());
host!(init, logic, AppState, {});
host!(init_runtime, logic, AppState, runtime: RuntimeHandle::std(), {});
host!(tick, logic, AppState, {}, |_: &AppState| {});
host!(tick_runtime, logic, AppState, runtime: RuntimeHandle::std(), {}, |_: &AppState| {});
host!(runtime_tick, logic, AppState, runtime: RuntimeHandle::std(), |_: &AppState| {});
