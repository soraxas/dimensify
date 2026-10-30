//! Browser file access only; all viewer presentation lives in the shared UI.
use super::{ViewerOptions, files::LocalFiles};
use crate::SourceUp;
use bevy::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Event, EventTarget, HtmlInputElement};

thread_local! {
    static EVENTS: RefCell<Vec<BrowserEvent>> = const { RefCell::new(Vec::new()) };
    static PICKER_UP: Cell<SourceUp> = const { Cell::new(SourceUp::Z) };
}

enum BrowserEvent {
    Files(Vec<web_sys::File>, Option<SourceUp>),
    Hover(bool),
    Cancel,
}

fn publish(event: BrowserEvent) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}

struct Listener {
    target: EventTarget,
    event: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            self.event,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}
fn listen(
    target: &EventTarget,
    event: &'static str,
    callback: impl FnMut(Event) + 'static,
) -> Result<Listener, JsValue> {
    let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
    target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())?;
    Ok(Listener {
        target: target.clone(),
        event,
        callback,
    })
}

struct BrowserFiles {
    input: HtmlInputElement,
    _listeners: Vec<Listener>,
}
impl Drop for BrowserFiles {
    fn drop(&mut self) {
        self.input.remove();
    }
}

fn install_handlers() -> Result<BrowserFiles, JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("Browser document unavailable")?;
    let input: HtmlInputElement = document.create_element("input")?.dyn_into()?;
    input.set_id("dimensify-file-input");
    input.set_type("file");
    input.set_accept(".ply,.sog");
    input.set_attribute("hidden", "")?;
    document
        .body()
        .ok_or("Browser body unavailable")?
        .append_child(&input)?;
    let selected = input.clone();
    let mut listeners = vec![
        listen(input.as_ref(), "change", move |_| {
            let files = selected
                .files()
                .map(|files| (0..files.length()).filter_map(|i| files.get(i)).collect())
                .unwrap_or_default();
            publish(BrowserEvent::Files(files, Some(PICKER_UP.with(Cell::get))));
            selected.set_value("");
        })?,
        listen(input.as_ref(), "cancel", |_| publish(BrowserEvent::Cancel))?,
    ];
    let depth = Rc::new(Cell::new(0_u32));
    let enter = depth.clone();
    listeners.push(listen(document.as_ref(), "dragenter", move |event| {
        event.prevent_default();
        enter.set(enter.get().saturating_add(1));
        publish(BrowserEvent::Hover(true));
    })?);
    let leave = depth.clone();
    listeners.push(listen(document.as_ref(), "dragleave", move |event| {
        event.prevent_default();
        leave.set(leave.get().saturating_sub(1));
        if leave.get() == 0 {
            publish(BrowserEvent::Hover(false));
        }
    })?);
    listeners.push(listen(document.as_ref(), "dragover", |event| {
        event.prevent_default();
        if let Some(transfer) = event
            .dyn_ref::<web_sys::DragEvent>()
            .and_then(|e| e.data_transfer())
        {
            transfer.set_drop_effect("copy");
        }
    })?);
    listeners.push(listen(document.as_ref(), "drop", move |event| {
        event.prevent_default();
        depth.set(0);
        publish(BrowserEvent::Hover(false));
        if let Some(files) = event
            .dyn_ref::<web_sys::DragEvent>()
            .and_then(|e| e.data_transfer())
            .and_then(|d| d.files())
        {
            publish(BrowserEvent::Files(
                (0..files.length()).filter_map(|i| files.get(i)).collect(),
                None,
            ));
        }
    })?);
    Ok(BrowserFiles {
        input,
        _listeners: listeners,
    })
}

/// Register browser file events once, retaining callbacks for the app lifetime.
pub(super) fn install(world: &mut World) {
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    if let Some((_, value)) = url::form_urlencoded::parse(query.trim_start_matches('?').as_bytes())
        .find(|(key, _)| key == "transparency")
    {
        match bevy_gaussian_splatting::GaussianTransparency::parse(&value) {
            Ok(mode) => world.resource_mut::<crate::runtime::SplatTransparency>().0 = mode,
            Err(error) => world.resource_mut::<LocalFiles>().notice = Some(error.into()),
        }
    }
    match install_handlers() {
        Ok(adapter) => world.insert_non_send(adapter),
        Err(error) => {
            world.resource_mut::<LocalFiles>().notice =
                Some(format!("Could not install file access: {error:?}"))
        }
    }
}

/// Open the browser system picker from a shared Feathers activation.
pub(super) fn choose(up: SourceUp) -> Result<(), JsValue> {
    PICKER_UP.with(|selected| selected.set(up));
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("Browser document unavailable")?;
    let input: HtmlInputElement = document
        .get_element_by_id("dimensify-file-input")
        .ok_or("File picker not ready")?
        .dyn_into()?;
    input.click();
    Ok(())
}

/// Route browser selections and drops through the common file-read state.
pub(super) fn receive(mut files: ResMut<LocalFiles>, options: Res<ViewerOptions>) {
    let events = EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
    for event in events {
        match event {
            BrowserEvent::Files(mut selected, up) if selected.len() == 1 => {
                files.open_browser(selected.remove(0), up.unwrap_or(options.up))
            }
            BrowserEvent::Files(selected, _) if selected.is_empty() => files.notice = None,
            BrowserEvent::Files(_, _) => {
                files.notice = Some("Choose one PLY or SOG file at a time".into())
            }
            BrowserEvent::Hover(hovering) => files.hovering = hovering,
            BrowserEvent::Cancel => files.notice = None,
        }
    }
}
