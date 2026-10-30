//! Browser codec jobs. The owning ChunkLoad is the scene/job lifetime;
//! its Drop cancels the worker and weak completion handles cannot revive it.
use bevy_gaussian_splatting::io::sog_packed::{self, DataTexture};
use js_sys::{Array, Object, Reflect, Uint8Array};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/streaming/runtime/codec_jobs.mjs")]
extern "C" {
    fn codec_requested(mode: Option<String>) -> bool;
    fn codec_unavailable_reason() -> String;
    fn codec_busy() -> bool;
    #[wasm_bindgen(catch)]
    fn codec_start(
        identity: &JsValue,
        files: &JsValue,
        limits: &JsValue,
        version: &str,
        expanded: &JsValue,
    ) -> Result<js_sys::Promise, JsValue>;
    fn codec_cancel(id: u32);
    fn codec_admitted(id: u32);
    fn codec_cancelled(id: u32) -> bool;
    fn codec_yield() -> js_sys::Promise;
}
static NEXT_JOB: AtomicU32 = AtomicU32::new(1);
const PEAK_ALLOWANCE: u64 = 128 * 1024 * 1024;

const fn fingerprint(bytes: &[u8], mut hash: u64) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u64).wrapping_mul(1099511628211);
        i += 1;
    }
    hash
}
// Compile-time only: source/lock text is not retained in the WASM payload.
const CODEC_VERSION: u64 = fingerprint(
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sog_codec/src/expanded.rs"
    )),
    fingerprint(
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")),
        fingerprint(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/streaming/runtime/sog-decode-worker.mjs"
            )),
            fingerprint(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../sog_codec/src/lib.rs"
                )),
                14695981039346656037,
            ),
        ),
    ),
);

/// Report whether the browser requested worker decoding.
pub fn requested() -> bool {
    codec_requested(
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .and_then(|search| {
                url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes())
                    .find(|(key, _)| key == "sog_decode")
                    .map(|(_, value)| value.into_owned())
            }),
    )
}
/// Describe the active codec mode and fallback capability.
pub fn description() -> String {
    if !requested() {
        return "Codec: serial.".into();
    }
    let reason = codec_unavailable_reason();
    if reason.is_empty() {
        "Codec: Rust worker (one job; validated source/texture envelope).".into()
    } else {
        format!("Codec: serial fallback — {reason}")
    }
}
/// Report worker capability and emit a bounded fallback warning.
pub fn available() -> bool {
    let reason = codec_unavailable_reason();
    if !reason.is_empty() {
        bevy::log::warn_once!("SOG codec worker unavailable; serial fallback: {reason}");
    }
    reason.is_empty()
}
/// Report whether a codec job or its admission owns the worker slot.
pub fn busy() -> bool {
    codec_busy()
}

// A bounded worker result is consumed once; keep its packed handoff inline.
#[allow(clippy::large_enum_variant)]
pub enum Decoded {
    Packed(sog_packed::PackedSog),
    Expanded(bevy_gaussian_splatting::PlanarGaussian3d),
}
type ResultSlot = Arc<Mutex<Option<Result<Decoded, String>>>>;
pub struct Job {
    id: u32,
    result: ResultSlot,
}
impl Drop for Job {
    fn drop(&mut self) {
        codec_cancel(self.id);
    }
}

fn set(object: &Object, key: &str, value: &JsValue) -> Result<(), String> {
    Reflect::set(object, &JsValue::from_str(key), value)
        .map(|_| ())
        .map_err(|e| format!("codec field {key}: {e:?}"))
}
fn get(object: &JsValue, key: &str) -> Result<JsValue, String> {
    Reflect::get(object, &JsValue::from_str(key)).map_err(|e| format!("codec result {key}: {e:?}"))
}
fn integer(object: &JsValue, key: &str) -> Result<u32, String> {
    get(object, key)?
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 1. && *n <= u32::MAX as f64 && n.fract() == 0.)
        .map(|n| n as u32)
        .ok_or_else(|| format!("invalid codec {key}"))
}
impl Job {
    /// Begin a bounded codec job or timing interval.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        scene: u64,
        source: &str,
        files: HashMap<String, Vec<u8>>,
        max_output: u64,
        meta: &bevy_gaussian_splatting::io::sog::SogMeta,
        expand: bool,
        flat: bool,
        max_source_splats: usize,
        cpu: crate::diagnostics::sog_cpu::SogCpuTimings,
    ) -> Result<Self, String> {
        let expanded_meta = expand.then_some(meta);
        let packed_meta = (!expand).then(|| meta.clone());
        if files.is_empty() || files.len() > 7 {
            return Err("invalid codec texture count".into());
        }
        // Inspect before crossing the JS/worker boundary or allocating pixels.
        let mut headers = Vec::new();
        let mut input = 0u64;
        let mut output = 0u64;
        let mut largest = 0u64;
        for (name, bytes) in &files {
            let header = sog_codec::inspect(
                bytes,
                if flat { 16384 } else { 8192 },
                max_output.saturating_sub(output),
            )
            .map_err(|e| e.to_string())?;
            input += bytes.len() as u64;
            output += header.rgba_bytes;
            largest = largest.max(header.rgba_bytes);
            headers.push((name, header));
        }
        use bevy_gaussian_splatting::material::spherical_harmonics::{
            SH_COEFF_COUNT, SH_COEFF_COUNT_PER_CHANNEL,
        };
        let expanded_count = expanded_meta
            .map(|meta| {
                meta.count
                    .checked_add(32 - meta.count % 32)
                    .ok_or("SOG padded count overflows")
            })
            .transpose()?;
        let expanded_bytes =
            expanded_count.map_or(0, |count| count as u64 * (12 + SH_COEFF_COUNT) as u64 * 4);
        let reserved = 4 * 1024 * 1024 + 4 * input + 2 * output + 8 * largest + 3 * expanded_bytes;
        let expanded = if let Some(meta) = expanded_meta {
            let value = Object::new();
            set(
                &value,
                "meta",
                &JsValue::from_str(&serde_json::to_string(meta).map_err(|e| e.to_string())?),
            )?;
            set(&value, "count", &JsValue::from_f64(meta.count as f64))?;
            set(
                &value,
                "coefficients",
                &JsValue::from_f64(SH_COEFF_COUNT as f64),
            )?;
            set(
                &value,
                "coefficientsPerChannel",
                &JsValue::from_f64(SH_COEFF_COUNT_PER_CHANNEL as f64),
            )?;
            JsValue::from(value)
        } else {
            JsValue::UNDEFINED
        };
        let allowance = if let Some(meta) = expanded_meta {
            if meta.count > max_source_splats
                || (!flat && files.values().any(|bytes| bytes.len() > 32 * 1024 * 1024))
            {
                return Err("SOG worker input exceeds source admission".into());
            }
            let source_rows = max_source_splats
                .checked_add(32 - max_source_splats % 32)
                .ok_or("SOG source admission overflows")?;
            let source_bytes = source_rows as u64 * (12 + SH_COEFF_COUNT) as u64 * 4;
            if source_bytes > isize::MAX as u64 || input > u32::MAX as u64 {
                return Err("SOG source exceeds WASM Vec allocation capacity".into());
            }
            // LOD source admission and count-bounded textures remain in force.
            // Flat has no cache admission: use checked format/addressability
            // bounds, without lowering the previously supported scene sizes.
            4 * 1024 * 1024 + 4 * input + 10 * output + 3 * source_bytes
        } else {
            PEAK_ALLOWANCE
        };
        if reserved > allowance {
            return Err(format!(
                "SOG worker needs {reserved} reserved bytes, exceeding its {} MiB limit; use ?sog_decode=serial for this scene (may pause navigation during decoding)",
                allowance / 1024 / 1024
            ));
        }
        let id = NEXT_JOB
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "SOG codec job identity exhausted")?;
        let identity = Object::new();
        set(&identity, "job", &JsValue::from(id))?;
        set(&identity, "scene", &JsValue::from_str(&scene.to_string()))?;
        set(&identity, "source", &JsValue::from_str(source))?;
        let encoded = Array::new();
        for (name, header) in headers {
            let bytes = files.get(name).ok_or("codec input disappeared")?;
            let file = Object::new();
            set(&file, "name", &JsValue::from_str(name))?;
            set(&file, "width", &JsValue::from(header.width))?;
            set(&file, "height", &JsValue::from(header.height))?;
            set(&file, "bytes", &Uint8Array::from(bytes.as_slice()).buffer())?;
            encoded.push(&file);
        }
        let limits = Object::new();
        set(
            &limits,
            "maxDimension",
            &JsValue::from(if flat { 16384 } else { 8192 }),
        )?;
        set(&limits, "flat", &JsValue::from_bool(flat))?;
        set(
            &limits,
            "maxSourceSplats",
            &JsValue::from_f64(max_source_splats as f64),
        )?;
        set(
            &limits,
            "maxOutputBytes",
            &JsValue::from_f64(max_output as f64),
        )?;
        set(
            &limits,
            "reservedPeakBytes",
            &JsValue::from_f64(reserved as f64),
        )?;
        let promise = codec_start(
            &identity,
            &encoded,
            &limits,
            &format!("{CODEC_VERSION:016x}"),
            &expanded,
        )
        .map_err(|e| format!("SOG codec start: {e:?}"))?;
        let result = Arc::new(Mutex::new(None));
        let weak = Arc::downgrade(&result);
        wasm_bindgen_futures::spawn_local(async move {
            let reply = wasm_bindgen_futures::JsFuture::from(promise).await;
            if weak.upgrade().is_none() {
                codec_admitted(id);
                return;
            }
            let decoded = match reply {
                Err(e) => Err(format!("SOG codec worker: {e:?}")),
                Ok(reply) => {
                    if let Some(count) = expanded_count {
                        let bytes = Uint8Array::new(&reply);
                        if bytes.length() as u64 != expanded_bytes {
                            Err("invalid SOG expanded byte length".into())
                        } else {
                            async {
                                let mut offset = 0;
                                let position_visibility =
                                    copy_plane(&bytes, count, &mut offset, &weak, &cpu, id).await?;
                                let rotation =
                                    copy_plane(&bytes, count, &mut offset, &weak, &cpu, id).await?;
                                let scale_opacity =
                                    copy_plane(&bytes, count, &mut offset, &weak, &cpu, id).await?;
                                let spherical_harmonic =
                                    copy_plane(&bytes, count, &mut offset, &weak, &cpu, id).await?;
                                Ok(Decoded::Expanded(
                                    bevy_gaussian_splatting::PlanarGaussian3d {
                                        position_visibility,
                                        rotation,
                                        scale_opacity,
                                        spherical_harmonic,
                                    },
                                ))
                            }
                            .await
                        }
                    } else {
                        admit_packed(reply, packed_meta.as_ref(), max_output, &weak, &cpu, id).await
                    }
                }
            };
            codec_admitted(id);
            let Some(slot) = weak.upgrade() else {
                return;
            };
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(decoded);
            }
        });
        Ok(Self { id, result })
    }
    /// Take a completed codec result once without blocking.
    pub fn poll(&self) -> Option<Result<Decoded, String>> {
        match self.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => Some(Err("SOG codec result lock poisoned".into())),
        }
    }
}

// Worker decoding is not the end of admission: copying and validating its
// reply must yield too. Keep the worker reservation until all of this finishes.
async fn admit_packed(
    reply: JsValue,
    meta: Option<&bevy_gaussian_splatting::io::sog::SogMeta>,
    max_output: u64,
    owner: &std::sync::Weak<Mutex<Option<Result<Decoded, String>>>>,
    cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
    id: u32,
) -> Result<Decoded, String> {
    let meta = meta.ok_or("missing packed source metadata")?;
    if !Array::is_array(&reply) {
        return Err("invalid codec texture list".into());
    }
    let items = Array::from(&reply);
    if items.length() == 0 || items.length() > 7 {
        return Err("invalid codec texture count".into());
    }
    let mut textures = HashMap::new();
    let mut remaining = max_output;
    for item in items.iter() {
        let name = get(&item, "name")?
            .as_string()
            .ok_or("codec result name is missing")?;
        let width = integer(&item, "width")?;
        let height = integer(&item, "height")?;
        if width > 8192 || height > 8192 {
            return Err("codec texture exceeds admitted dimensions".into());
        }
        let bytes = Uint8Array::new(&get(&item, "pixels")?);
        let length = u64::from(width) * u64::from(height) * 4;
        if length != u64::from(bytes.length()) || length > remaining {
            return Err("codec texture exceeds admitted dimensions/bytes".into());
        }
        remaining -= length;
        let mut offset = 0;
        let pixels =
            copy_plane::<u8>(&bytes, bytes.length() as usize, &mut offset, owner, cpu, id).await?;
        let texture = cpu.measure(crate::diagnostics::sog_cpu::CpuStage::Decode, || {
            DataTexture::from_rgba(width, height, pixels).map_err(|e| e.to_string())
        })?;
        if textures.insert(name, texture).is_some() {
            return Err("duplicate codec texture".into());
        }
    }
    let mut validation = cpu.measure(crate::diagnostics::sog_cpu::CpuStage::Decode, || {
        sog_packed::begin_decode_rgba(
            meta,
            &textures,
            3,
            sog_packed::DecodeLimits {
                max_texture_dimension: 8192,
                max_texture_bytes: max_output,
            },
        )
        .map_err(|e| e.to_string())
    })?;
    drop(textures); // validation owns shared texels; no extra copy
    loop {
        if owner.upgrade().is_none() || codec_cancelled(id) {
            return Err("SOG codec admission cancelled".into());
        }
        let source = cpu.measure(crate::diagnostics::sog_cpu::CpuStage::Decode, || {
            validation.advance(32 * 1024).map_err(|e| e.to_string())
        })?;
        if let Some(source) = source {
            return Ok(Decoded::Packed(source));
        }
        let _ = wasm_bindgen_futures::JsFuture::from(codec_yield()).await;
    }
}

// Reserve once, but initialize and copy at most 1 MiB before a browser macrotask.
// Keep only a weak scene owner so cancellation retires partial planes too.
async fn copy_plane<T: bytemuck::Pod + Default>(
    bytes: &Uint8Array,
    count: usize,
    offset: &mut u32,
    owner: &std::sync::Weak<Mutex<Option<Result<Decoded, String>>>>,
    cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
    id: u32,
) -> Result<Vec<T>, String> {
    let mut plane = cpu.measure(crate::diagnostics::sog_cpu::CpuStage::Decode, || {
        Vec::with_capacity(count)
    });
    let batch = (1024 * 1024 / std::mem::size_of::<T>()).max(1);
    while plane.len() < count {
        if owner.upgrade().is_none() || codec_cancelled(id) {
            return Err("SOG codec admission cancelled".into());
        }
        {
            let _timer = cpu.start(crate::diagnostics::sog_cpu::CpuStage::Decode);
            let start = plane.len();
            plane.resize((start + batch).min(count), T::default());
            let target = bytemuck::cast_slice_mut(&mut plane[start..]);
            let end = *offset + target.len() as u32;
            bytes.subarray(*offset, end).copy_to(target);
            *offset = end;
        }
        let _ = wasm_bindgen_futures::JsFuture::from(codec_yield()).await;
    }
    Ok(plane)
}
