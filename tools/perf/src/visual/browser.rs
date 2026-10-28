use super::*;
use wasm_bindgen::JsValue;
fn publish(value: Value) {
    let window = web_sys::window().expect("browser window");
    let parsed = js_sys::JSON::parse(&value.to_string()).expect("serializable visual status");
    js_sys::Reflect::set(
        window.as_ref(),
        &JsValue::from_str("__DIMENSIFY_VISUAL__"),
        &parsed,
    )
    .expect("publish visual status");
}
#[derive(Resource)]
struct State {
    name: String,
    target: Option<Handle<Image>>,
    stable: u32,
    frames: u32,
    requested: bool,
    done: bool,
}
pub fn run() {
    std::panic::set_hook(Box::new(|error| {
        publish(json!({"schema":"dimensify-visual-status/1",
        "status":"failed","error":error.to_string()}))
    }));
    let query = web_sys::window()
        .unwrap()
        .location()
        .search()
        .unwrap_or_default();
    let name = query
        .trim_start_matches('?')
        .split('&')
        .find_map(|part| part.strip_prefix("case="))
        .unwrap_or("")
        .to_owned();
    if !CASES.contains(&name.as_str()) {
        publish(json!({"status":"failed","error":"URL requires a known ?case=NAME"}));
        return;
    }
    publish(json!({"schema":"dimensify-visual-status/1","status":"loading","case":name}));
    let mut app = App::new();
    plugins(&mut app);
    app.init_resource::<Captured>()
        .insert_resource(State {
            name,
            target: None,
            stable: 0,
            frames: 0,
            requested: false,
            done: false,
        })
        .add_systems(Update, tick)
        .run();
}
fn tick(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<State>| {
        if state.done {
            return;
        }
        state.frames += 1;
        if state.frames > 3600 {
            publish(json!({"status":"failed","error":"production readiness/capture timeout"}));
            state.done = true;
            return;
        }
        if state.target.is_none() {
            state.target = Some(setup_scene(world, &state.name).0);
            return;
        }
        state.stable = if acknowledged(world) {
            state.stable + 1
        } else {
            0
        };
        if state.stable >= 3 && !state.requested {
            request_capture(world, state.target.clone().unwrap());
            state.requested = true;
        }
        let Some(image) = world.resource_mut::<Captured>().0.take() else {
            return;
        };
        let result = pixels(image).and_then(|rgba| {
            expectations::check(&state.name, &rgba)?;
            Ok(rgba)
        });
        match result {
            Ok(rgba) => {
                let metadata = metadata(world, "browser-webgpu");
                publish(json!({
                    "schema":"dimensify-visual-status/1", "status":"complete", "case":state.name,
                    "ready":true, "rendered":true, "width":SIZE, "height":SIZE, "metadata":metadata,
                    "captures":[capture_row(&state.name)], "rgba":rgba,
                    "semanticChecks":{"passed":true}, "errors":[]
                }));
            }
            Err(error) => publish(json!({"status":"failed","error":error,"case":state.name})),
        }
        state.done = true;
    });
}
