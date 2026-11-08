//! Bounded network delivery owned by one viewer selection.
use super::decoder::{Batch, Decoder};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};

pub(crate) enum Event {
    Batch(Batch),
    Complete,
    Fallback(Vec<u8>),
    Failed(String),
}
#[derive(Default)]
struct Queue {
    events: VecDeque<Event>,
    prefix: Vec<u8>,
}
/// Dropping the receiver retires queued batches and prevents further publication.
pub(crate) struct Download(Arc<Mutex<Queue>>);
struct Producer {
    queue: Weak<Mutex<Queue>>,
    decoder: Decoder,
    received: usize,
}
#[cfg(target_arch = "wasm32")]
thread_local! {
    // The viewer's fetches and receiver teardown run on the browser thread.
    // Keep JS controllers outside Send/Sync Bevy resources.
    static ABORTS: std::cell::RefCell<std::collections::HashMap<usize,web_sys::AbortController>> = std::cell::RefCell::new(std::collections::HashMap::new());
}
#[cfg(target_arch = "wasm32")]
impl Drop for Download {
    fn drop(&mut self) {
        ABORTS.with(|controllers| {
            if let Some(controller) = controllers
                .borrow_mut()
                .remove(&(Arc::as_ptr(&self.0) as usize))
            {
                controller.abort();
            }
        });
    }
}
impl Download {
    fn channel(storage_limit: Option<u64>) -> (Self, Producer) {
        let queue = Arc::new(Mutex::new(Queue::default()));
        let producer = Producer {
            queue: Arc::downgrade(&queue),
            decoder: Decoder::new(storage_limit),
            received: 0,
        };
        (Self(queue), producer)
    }
    pub(crate) fn start(url: String, storage_limit: Option<u64>) -> Self {
        let (download, mut producer) = Self::channel(storage_limit);
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            if let Err(error) = native_fetch(&url, &mut producer) {
                producer.send(Event::Failed(error));
            }
        });
        #[cfg(target_arch = "wasm32")]
        match web_sys::AbortController::new() {
            Ok(controller) => {
                let signal = controller.signal();
                let key = Arc::as_ptr(&download.0) as usize;
                ABORTS.with(|controllers| {
                    controllers.borrow_mut().insert(key, controller);
                });
                wasm_bindgen_futures::spawn_local(async move {
                    let result = browser_fetch(&url, &mut producer, &signal).await;
                    ABORTS.with(|controllers| {
                        controllers.borrow_mut().remove(&key);
                    });
                    if let Err(error) = result {
                        producer.send(Event::Failed(error)).await;
                    }
                });
            }
            Err(_) => {
                let _ = producer.try_send(Event::Failed(
                    "Could not initialize cancellable fetch".into(),
                ));
            }
        }
        download
    }
    pub(crate) fn pop(&self) -> Option<Event> {
        self.0.lock().unwrap().events.pop_front()
    }
}
impl Producer {
    fn alive(&self) -> bool {
        self.queue.strong_count() != 0
    }
    fn try_send(&self, event: Event) -> Option<Event> {
        let Some(queue) = self.queue.upgrade() else {
            return Some(event);
        };
        let mut queue = queue.lock().unwrap();
        if queue.events.len() >= 2 {
            return Some(event);
        }
        queue.events.push_back(event);
        None
    }
    // A read is limited to 64 KiB before decoding. Only two decoded events can
    // wait for the viewer; network reading pauses when that queue is full.
    #[cfg(not(target_arch = "wasm32"))]
    fn send(&self, mut event: Event) -> bool {
        while self.alive() {
            match self.try_send(event) {
                None => return true,
                Some(pending) => event = pending,
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        false
    }
    #[cfg(target_arch = "wasm32")]
    async fn send(&self, mut event: Event) -> bool {
        while self.alive() {
            match self.try_send(event) {
                None => return true,
                Some(pending) => event = pending,
            }
            pause().await;
        }
        false
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<Vec<Event>, String> {
        self.received = self
            .received
            .checked_add(bytes.len())
            .ok_or("PLY input size overflow")?;
        if self.received > crate::decode::MAX_BYTES {
            return Err("Download exceeds the 256 MiB input limit".into());
        }
        let queue = self.queue.upgrade().ok_or("PLY selection was canceled")?;
        if !self.decoder.is_decided() || self.decoder.is_fallback() {
            queue.lock().unwrap().prefix.extend_from_slice(bytes);
        }
        let batches = self.decoder.push(bytes)?;
        if self.decoder.is_decided() && !self.decoder.is_fallback() {
            queue.lock().unwrap().prefix = Vec::new();
        }
        Ok(batches.into_iter().map(Event::Batch).collect())
    }
    fn finish(&mut self) -> Result<Event, String> {
        self.decoder.finish()?;
        if self.decoder.is_fallback() {
            let queue = self.queue.upgrade().ok_or("PLY selection was canceled")?;
            let bytes = std::mem::take(&mut queue.lock().unwrap().prefix);
            Ok(Event::Fallback(bytes))
        } else {
            Ok(Event::Complete)
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn native_fetch(url: &str, producer: &mut Producer) -> Result<(), String> {
    use std::io::Read;
    let client = reqwest::blocking::Client::builder()
        .referer(false)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "Could not start HTTP client")?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|_| "Download failed (connection, TLS or timeout)")?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > crate::decode::MAX_BYTES as u64)
    {
        return Err("Download exceeds the 256 MiB input limit".into());
    }
    let mut buffer = [0; 64 * 1024];
    while producer.alive() {
        let count = response
            .read(&mut buffer)
            .map_err(|_| "Could not read the HTTP response")?;
        if count == 0 {
            let event = producer.finish()?;
            producer.send(event);
            return Ok(());
        }
        for event in producer.decode(&buffer[..count])? {
            if !producer.send(event) {
                return Ok(());
            }
        }
    }
    Ok(())
}
#[cfg(target_arch = "wasm32")]
async fn pause() {
    use wasm_bindgen::JsCast;
    // A timer yields to Bevy's animation updates while applying backpressure.
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(resolve.unchecked_ref(), 4);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
#[cfg(target_arch = "wasm32")]
async fn browser_fetch(
    url: &str,
    producer: &mut Producer,
    signal: &web_sys::AbortSignal,
) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or("No browser window")?;
    let options = web_sys::RequestInit::new();
    options.set_signal(Some(signal));
    let request = web_sys::Request::new_with_str_and_init(url, &options)
        .map_err(|_| "Could not start HTTP request")?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Download failed; check the URL, HTTPS and the host's CORS settings")?
        .dyn_into()
        .map_err(|_| "Invalid HTTP response")?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|n| n.parse::<u64>().ok())
        .is_some_and(|n| n > crate::decode::MAX_BYTES as u64)
    {
        return Err("Download exceeds the 256 MiB input limit".into());
    }
    let reader: web_sys::ReadableStreamDefaultReader = response
        .body()
        .ok_or("HTTP response has no body")?
        .get_reader()
        .dyn_into()
        .map_err(|_| "Could not read HTTP stream")?;
    let result = async {
        while producer.alive() {
            let result = JsFuture::from(reader.read())
                .await
                .map_err(|_| "Could not read the HTTP response")?;
            let done = js_sys::Reflect::get(&result, &"done".into())
                .map_err(|_| "Invalid HTTP stream")?
                .as_bool()
                .unwrap_or(false);
            if done {
                let event = producer.finish()?;
                producer.send(event).await;
                return Ok(());
            }
            let value =
                js_sys::Reflect::get(&result, &"value".into()).map_err(|_| "Invalid HTTP chunk")?;
            let chunk = js_sys::Uint8Array::new(&value);
            if chunk.length() as usize > crate::decode::MAX_BYTES.saturating_sub(producer.received)
            {
                return Err("Download exceeds the 256 MiB input limit".into());
            }
            // Browser chunks can be large; bound each decode/copy and yield so
            // a fast cached response cannot monopolize the animation thread.
            for offset in (0..chunk.length()).step_by(64 * 1024) {
                if !producer.alive() {
                    return Ok(());
                }
                let bytes = chunk
                    .subarray(offset, offset.saturating_add(64 * 1024).min(chunk.length()))
                    .to_vec();
                for event in producer.decode(&bytes)? {
                    if !producer.send(event).await {
                        return Ok(());
                    }
                }
                pause().await;
            }
        }
        Ok(())
    }
    .await;
    let _ = reader.cancel();
    result
}
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "transport_tests.rs"]
mod tests;
