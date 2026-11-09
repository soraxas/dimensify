//! Browser file handles and DOM events; presentation stays in Feathers.
use super::*;
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};
thread_local! { static EVENTS: RefCell<Vec<JsValue>> = const { RefCell::new(Vec::new()) }; }
#[wasm_bindgen(module = "/src/viewer/open/browser_files.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = installFiles)]
    fn install_files(callback: &js_sys::Function) -> Result<js_sys::Function, JsValue>;
    #[wasm_bindgen(catch, js_name = chooseFiles)]
    fn choose_files(folder: bool, up: u8, ticket: f64) -> Result<(), JsValue>;
}
struct BrowserFiles {
    dispose: js_sys::Function,
    _callback: Closure<dyn FnMut(JsValue) -> JsValue>,
}
impl Drop for BrowserFiles {
    fn drop(&mut self) {
        let _ = self.dispose.call0(&JsValue::NULL);
    }
}
fn property(value: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(value, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
pub(super) fn install(world: &mut World) {
    let mut drop_id = 0_u64;
    let callback = Closure::wrap(Box::new(move |event: JsValue| -> JsValue {
        let result = if property(&event, "kind").as_string().as_deref() == Some("beginDrop") {
            drop_id += 1;
            let _ =
                js_sys::Reflect::set(&event, &"ticket".into(), &JsValue::from_f64(drop_id as f64));
            JsValue::from_f64(drop_id as f64)
        } else {
            JsValue::UNDEFINED
        };
        EVENTS.with(|events| events.borrow_mut().push(event));
        result
    }) as Box<dyn FnMut(JsValue) -> JsValue>);
    match install_files(callback.as_ref().unchecked_ref()) {
        Ok(dispose) => world.insert_non_send(BrowserFiles {
            dispose,
            _callback: callback,
        }),
        Err(error) => {
            world.resource_mut::<files::Files>().notice =
                Some(format!("Cannot install browser file access: {error:?}"))
        }
    }
}
pub(super) fn choose(folder: bool, up: PointcloudUp, ticket: u64) -> Result<(), JsValue> {
    choose_files(folder, u8::from(up == PointcloudUp::Z), ticket as f64)
}
pub(super) fn receive(
    mut files: ResMut<files::Files>,
    panel: Res<OpenPanel>,
    mut drop: Local<Option<(u64, u64, PointcloudUp)>>,
) {
    let events = EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
    for event in events {
        let kind = property(&event, "kind").as_string().unwrap_or_default();
        let ticket = property(&event, "ticket").as_f64().unwrap_or_default() as u64;
        match kind.as_str() {
            "hover" => files.hovering = property(&event, "value").as_bool().unwrap_or(false),
            "beginDrop" => {
                files.invalidate();
                files.notice = Some("Reading dropped folder...".into());
                *drop = Some((ticket, files.ticket, panel.up));
            }
            "dropError" => {
                if drop.is_some_and(|(id, selection, _)| id == ticket && selection == files.ticket)
                {
                    files.notice = Some(
                        property(&event, "message")
                            .as_string()
                            .unwrap_or_else(|| "Cannot read dropped folder".into()),
                    );
                }
            }
            "files" | "dropFiles" => {
                let (selection, up) = if kind == "dropFiles" {
                    let Some((id, selection, up)) = *drop else {
                        continue;
                    };
                    if id != ticket {
                        continue;
                    }
                    (selection, up)
                } else {
                    (
                        ticket,
                        if property(&event, "up").as_f64() == Some(1.) {
                            PointcloudUp::Z
                        } else {
                            PointcloudUp::Y
                        },
                    )
                };
                let selected = js_sys::Array::from(&property(&event, "files"))
                    .iter()
                    .filter_map(|value| {
                        Some((
                            property(&value, "path").as_string()?,
                            crate::PointcloudFile::from_browser(
                                property(&value, "file").dyn_into::<web_sys::File>().ok()?,
                            ),
                        ))
                    })
                    .collect();
                files.browser_selection(Some(selection), selected, up);
            }
            _ => {}
        }
    }
}
