//! Bounded immutable parameter batches. Dynamic offsets reuse one destination
//! bind group across ranges; no queue writes can alias deferred view parameters.
use super::{BakeDiagnostics, BakeRange, CompactBaker, CompactWork, GpuSogSource, LocalView};
use wgpu::util::DeviceExt;

const PARAM_BYTES: u64 = 48;
const MAX_BATCH_BYTES: u64 = 16 * 1024;
const MAX_BATCH_RANGES: u64 = 64;

struct BatchLayout {
    stride: u64,
    capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_layout_respects_alignment_capacity_and_device_limits() {
        // Literal limits exercise byte-bound batches, not just the range cap.
        for (alignment, max_buffer_size, dynamic, stride, capacity) in [
            (256, 65536, true, 256, 64),
            (512, 65536, true, 512, 32),
            (32, 65536, true, 64, 64),
            (256, 1024, true, 256, 4),
            (256, 48, true, 256, 1),
            (256, 65536, false, 256, 1),
            (65536, 65536, true, 65536, 1),
        ] {
            let limits = wgpu::Limits {
                min_uniform_buffer_offset_alignment: alignment,
                max_buffer_size,
                ..Default::default()
            };
            let layout = BatchLayout::new(&limits, dynamic).unwrap();
            assert_eq!((layout.stride, layout.capacity), (stride, capacity));
            assert!((capacity as u64 - 1) * stride + 48 <= max_buffer_size.min(16384));
        }
        assert!(
            BatchLayout::new(
                &wgpu::Limits {
                    max_buffer_size: 47,
                    ..Default::default()
                },
                true
            )
            .is_err()
        );
    }
}

impl BatchLayout {
    fn new(limits: &wgpu::Limits, dynamic: bool) -> Result<Self, &'static str> {
        let max_bytes = limits.max_buffer_size.min(MAX_BATCH_BYTES);
        if max_bytes < PARAM_BYTES {
            return Err("device cannot allocate SOG bake parameters");
        }
        let alignment = u64::from(limits.min_uniform_buffer_offset_alignment.max(1));
        let stride = PARAM_BYTES.div_ceil(alignment) * alignment;
        let capacity = if dynamic {
            ((max_bytes - PARAM_BYTES) / stride + 1).min(MAX_BATCH_RANGES) as usize
        } else {
            1
        };
        Ok(Self { stride, capacity })
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn encode<'a>(
    baker: &CompactBaker,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    work: &CompactWork,
    ranges: impl IntoIterator<Item = (&'a GpuSogSource, BakeRange)>,
    view: LocalView,
    color_only: bool,
    diagnostics: &BakeDiagnostics,
) -> Result<(), &'static str> {
    let limits = device.limits();
    let layout = BatchLayout::new(&limits, baker.dynamic_uniforms)?;
    let mut batch = Vec::with_capacity(layout.capacity);
    for (source, range) in ranges {
        range.validate(source.count(), work.capacity())?;
        if range.count == 0 {
            continue;
        }
        if range.count.div_ceil(64) > limits.max_compute_workgroups_per_dimension {
            return Err("split bake range into device-supported dispatch sizes");
        }
        batch.push((source, range));
        if batch.len() == layout.capacity {
            encode_batch(
                baker,
                device,
                encoder,
                work,
                &batch,
                &layout,
                view,
                color_only,
                diagnostics,
            );
            batch.clear();
        }
    }
    if !batch.is_empty() {
        encode_batch(
            baker,
            device,
            encoder,
            work,
            &batch,
            &layout,
            view,
            color_only,
            diagnostics,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_batch(
    baker: &CompactBaker,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    work: &CompactWork,
    batch: &[(&GpuSogSource, BakeRange)],
    layout: &BatchLayout,
    view: LocalView,
    color_only: bool,
    diagnostics: &BakeDiagnostics,
) {
    // Only called for a nonempty batch fitting both count and byte bounds.
    let size = (batch.len() as u64 - 1) * layout.stride + PARAM_BYTES;
    let mut params = vec![0u8; size as usize];
    for (i, (_, range)) in batch.iter().enumerate() {
        let start = i * layout.stride as usize;
        let header = [
            range.source_start,
            range.count,
            range.destination_start,
            u32::from(color_only),
        ];
        params[start..start + 16].copy_from_slice(bytemuck::cast_slice(&header));
        params[start + 16..start + 32].copy_from_slice(bytemuck::cast_slice(&view.position));
        params[start + 32..start + 48].copy_from_slice(bytemuck::cast_slice(&view.direction));
    }
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("immutable SOG bake batch parameters"),
        contents: &params,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let [transform, rotation, color] = work.buffers();
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("compact SOG batch destination"),
        layout: &baker.work_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: transform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: rotation.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: color.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: std::num::NonZeroU64::new(PARAM_BYTES),
                }),
            },
        ],
    });
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("SOG range batch bake"),
        timestamp_writes: None,
    });
    pass.set_pipeline(&baker.pipeline);
    for (i, (source, range)) in batch.iter().enumerate() {
        // Byte bound is <=16 KiB, so each offset fits u32 on native and WASM.
        let offset = [(i as u64 * layout.stride) as u32];
        pass.set_bind_group(0, source.bind_group(), &[]);
        pass.set_bind_group(
            1,
            &group,
            if baker.dynamic_uniforms { &offset } else { &[] },
        );
        pass.dispatch_workgroups(range.count.div_ceil(64), 1, 1);
    }
    // Counts mean encoded work, not GPU completion, live memory or duration.
    diagnostics.record(|report| {
        let rows = if color_only {
            &mut report.color_rows
        } else {
            &mut report.full_rows
        };
        for (_, range) in batch {
            *rows = rows.saturating_add(u64::from(range.count));
        }
        report.dispatches = report.dispatches.saturating_add(batch.len() as u64);
        report.uniform_buffers = report.uniform_buffers.saturating_add(1);
        report.uniform_bytes = report.uniform_bytes.saturating_add(uniform.size());
        report.bind_groups = report.bind_groups.saturating_add(1);
        report.compute_passes = report.compute_passes.saturating_add(1);
    });
}
