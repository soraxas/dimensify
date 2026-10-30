//! Optional Metal device resource accounting; never physical GPU residency.
//!
//! Apple defines currentAllocatedSize as bytes used by resources of that device:
//! https://developer.apple.com/documentation/metal/mtldevice/currentallocatedsize
//! Name equality alone cannot prove that the system-default device counter covers
//! wgpu's device. A strict known-allocation delta probe must pass each run first.

use bevy::render::renderer::RenderDevice;
use serde_json::{Value, json};
#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(any(target_os = "macos", test))]
const PROBE_BYTES: u64 = 1024 * 1024;
const METHOD: &str = "MTLCreateSystemDefaultDevice().currentAllocatedSize()";
const SCOPE: &str = "process system-default Metal device resource allocations; active-wgpu-device coverage requires allocation_probe.passed; includes resources beyond buffers; not physical VRAM or driver residency";

#[cfg(target_os = "macos")]
static VERIFIED: Mutex<Option<(String, Value)>> = Mutex::new(None);

fn result(
    bytes: Option<u64>,
    name: Option<&str>,
    reason: Option<&str>,
    probe: Option<Value>,
) -> Value {
    json!({
        "metric": "metal_device_resource_allocated_bytes",
        "bytes": bytes,
        "method": METHOD,
        "matched_device_name": name,
        "reason": reason,
        "allocation_probe": probe,
        "scope": SCOPE,
    })
}

#[cfg(any(target_os = "macos", test))]
fn validate_reading(
    adapter_name: &str,
    backend: &str,
    name: &str,
    bytes: u64,
) -> Result<(), &'static str> {
    if backend != "Metal" {
        return Err("Metal counter does not describe the active backend");
    }
    if name.is_empty() || adapter_name != name {
        return Err("System-default Metal device name does not exactly match active adapter");
    }
    if bytes == 0 {
        return Err("Metal resource counter is zero despite active-device allocation probe");
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn validates_delta(before: u64, after: u64) -> bool {
    after.checked_sub(before) == Some(PROBE_BYTES)
}

#[cfg(target_os = "macos")]
fn counter() -> Option<(String, u64)> {
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};
    let device = MTLCreateSystemDefaultDevice()?;
    Some((
        device.name().to_string(),
        device.currentAllocatedSize() as u64,
    ))
}

/// Validate counter coverage using an actual 1 MiB wgpu buffer allocation.
///
/// Call once after renderer setup, before timed work. A noisy, unsupported or
/// ambiguous delta leaves accounting unavailable. The probe does not establish
/// physical memory residency or GPU allocation isolation from other work.
#[cfg(target_os = "macos")]
pub fn validate_active_device(device: &RenderDevice, adapter_name: &str, backend: &str) -> Value {
    use bevy::render::render_resource::{BufferDescriptor, BufferUsages};
    let mut verified = VERIFIED.lock().unwrap_or_else(|poison| poison.into_inner());
    *verified = None;
    if backend != "Metal" {
        return result(None, None, Some("Active backend is not Metal"), None);
    }
    let Some((name, before)) = counter() else {
        return result(
            None,
            None,
            Some("System-default Metal device unavailable"),
            None,
        );
    };
    if name != adapter_name || name.is_empty() {
        return result(
            None,
            Some(&name),
            Some("System-default Metal device name does not exactly match active adapter"),
            None,
        );
    }
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("dimensify-perf-device-counter-probe"),
        size: PROBE_BYTES,
        usage: BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let after = counter();
    // Keep the allocation alive until the second counter read.
    drop(buffer);
    let Some((after_name, after)) = after else {
        return result(
            None,
            Some(&name),
            Some("Metal device unavailable after allocation probe"),
            None,
        );
    };
    let valid = after_name == name && validates_delta(before, after);
    let probe = json!({"requested_bytes":PROBE_BYTES,"before_bytes":before,"after_bytes":after,"observed_delta_bytes":after.checked_sub(before),"passed":valid,"criterion":"exact increase equal to known wgpu buffer size; fail closed on noise or different device"});
    if !valid {
        return result(
            None,
            Some(&name),
            Some(
                "System-default Metal counter did not exactly follow active-wgpu-device allocation; counter coverage unverified",
            ),
            Some(probe),
        );
    }
    *verified = Some((name.clone(), probe));
    drop(verified);
    snapshot(adapter_name, backend)
}

#[cfg(not(target_os = "macos"))]
pub fn validate_active_device(_device: &RenderDevice, adapter_name: &str, backend: &str) -> Value {
    snapshot(adapter_name, backend)
}

/// Sample only a previously verified matching Metal allocation counter.
#[cfg(target_os = "macos")]
pub fn snapshot(adapter_name: &str, backend: &str) -> Value {
    let verified = VERIFIED.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some((verified_name, probe)) = verified.as_ref() else {
        return result(
            None,
            None,
            Some("Active-device allocation coverage has not passed the runtime probe"),
            None,
        );
    };
    let Some((name, bytes)) = counter() else {
        return result(
            None,
            None,
            Some("System-default Metal device unavailable"),
            Some(probe.clone()),
        );
    };
    let reason = validate_reading(adapter_name, backend, &name, bytes)
        .err()
        .or_else(|| (verified_name != &name).then_some("Device changed since allocation probe"));
    result(
        reason.is_none().then_some(bytes),
        Some(&name),
        reason,
        Some(probe.clone()),
    )
}

#[cfg(not(target_os = "macos"))]
pub fn snapshot(_adapter_name: &str, _backend: &str) -> Value {
    result(
        None,
        None,
        Some("Supported device-resource counter is available only on native macOS Metal"),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_positive_matching_metal_counter_is_accepted() {
        assert!(validate_reading("Apple M4", "Metal", "Apple M4", 1).is_ok());
        assert!(validate_reading("Apple M4", "Vulkan", "Apple M4", 1).is_err());
        assert!(validate_reading("Apple M4", "Metal", "Apple M3", 1).is_err());
        assert!(validate_reading("", "Metal", "", 1).is_err());
        assert!(validate_reading("Apple M4", "Metal", "Apple M4", 0).is_err());
    }

    #[test]
    fn allocation_probe_fails_closed_on_missing_noisy_or_decreasing_counter() {
        assert!(validates_delta(128, 128 + PROBE_BYTES));
        assert!(!validates_delta(128, 128));
        assert!(!validates_delta(128, 128 + PROBE_BYTES - 1));
        assert!(!validates_delta(128, 128 + PROBE_BYTES + 1));
        assert!(!validates_delta(128, 0));
        assert!(!validates_delta(u64::MAX, 0));
    }

    #[test]
    fn unavailable_counter_preserves_null_without_residency_claim() {
        let value = result(None, Some("Apple M4"), Some("probe failed"), None);
        assert!(value["bytes"].is_null());
        assert_eq!(value["metric"], "metal_device_resource_allocated_bytes");
        assert_eq!(value["reason"], "probe failed");
        assert!(
            value["scope"]
                .as_str()
                .unwrap()
                .contains("not physical VRAM")
        );
    }
}
