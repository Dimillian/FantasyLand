//! Optional pass-boundary timestamps. Readback never waits for the GPU.
use serde::Serialize;
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
const PAIRS: u32 = 32;
const BYTES: u64 = PAIRS as u64 * 16;
pub const NAMES: [&str; 29] = [
    "Sun shadows",
    "Enclosure",
    "Reflections",
    "World + foliage",
    "Water + rain",
    "Godrays march",
    "Godrays reconstruction",
    "Bloom extract",
    "Bloom blur X",
    "Bloom blur Y",
    "Bloom downsample 1",
    "Bloom blur X1",
    "Bloom blur Y1",
    "Bloom downsample 2",
    "Bloom blur X2",
    "Bloom blur Y2",
    "Presentation",
    "Reserved",
    "AO depth",
    "AO horizons",
    "AO denoise",
    "AO upsample",
    "Far sun cascade",
    "Indirect probe updates",
    "Meadow generation",
    "Cloud cache",
    "Water solver",
    "GI scroll",
    "GI screen cache",
];
#[derive(Clone, Debug, Serialize)]
pub struct PassTime {
    pub name: String,
    pub ms: f32,
}
pub struct GpuProfile {
    queries: Option<wgpu::QuerySet>,
    resolve: Option<wgpu::Buffer>,
    read: Option<wgpu::Buffer>,
    busy: Arc<AtomicBool>,
    result: Arc<Mutex<(Vec<PassTime>, Option<f32>, u32)>>,
    active: bool,
    slots: Cell<[u32; PAIRS as usize]>,
    used: Cell<u32>,
    frame: u32,
    period: f32,
}
impl GpuProfile {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        Self {
            queries: supported.then(|| {
                device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Sampled renderer GPU timings"),
                    ty: wgpu::QueryType::Timestamp,
                    count: PAIRS * 2,
                })
            }),
            resolve: supported.then(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GPU timing resolve"),
                    size: BYTES,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                })
            }),
            read: supported.then(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Asynchronous GPU timing readback"),
                    size: BYTES,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
            busy: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new((vec![], None, 0))),
            active: false,
            slots: Cell::new([u32::MAX; PAIRS as usize]),
            used: Cell::new(0),
            frame: 0,
            period: queue.get_timestamp_period(),
        }
    }
    pub fn begin(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.active =
            self.queries.is_some() && self.frame % 30 == 0 && !self.busy.load(Ordering::Relaxed);
        self.slots.set([u32::MAX; PAIRS as usize]);
        self.used.set(0);
    }
    pub fn pass(&self, pair: u32) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        if !self.active || pair >= PAIRS {
            return None;
        }
        let index = self.used.get();
        if index >= PAIRS * 2 {
            return None;
        }
        let mut slots = self.slots.get();
        slots[pair as usize] = index;
        self.slots.set(slots);
        self.used.set(index + 2);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: self.queries.as_ref()?,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: Some(index + 1),
        })
    }
    pub fn compute_pass(&self, pair: u32) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        if !self.active || pair >= PAIRS {
            return None;
        }
        let index = self.used.get();
        if index >= PAIRS * 2 {
            return None;
        }
        let mut slots = self.slots.get();
        slots[pair as usize] = index;
        self.slots.set(slots);
        self.used.set(index + 2);
        Some(wgpu::ComputePassTimestampWrites {
            query_set: self.queries.as_ref()?,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: Some(index + 1),
        })
    }
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.active {
            return;
        }
        encoder.resolve_query_set(
            self.queries.as_ref().unwrap(),
            0..self.used.get(),
            self.resolve.as_ref().unwrap(),
            0,
        );
        encoder.copy_buffer_to_buffer(
            self.resolve.as_ref().unwrap(),
            0,
            self.read.as_ref().unwrap(),
            0,
            self.used.get() as u64 * 8,
        );
    }
    pub fn readback(&self) {
        if !self.active {
            return;
        }
        let buffer = self.read.as_ref().unwrap().clone();
        let busy = self.busy.clone();
        let result = self.result.clone();
        let slots = self.slots.get();
        let period = self.period;
        let sample_id = self.frame;
        busy.store(true, Ordering::Relaxed);
        self.read
            .as_ref()
            .unwrap()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |status| {
                if status.is_ok() {
                    let bytes = buffer.slice(..).get_mapped_range();
                    let words: Vec<u64> = bytes
                        .chunks_exact(8)
                        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    let values = NAMES
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| slots[*i] != u32::MAX)
                        .filter_map(|(i, name)| {
                            let index = slots[i] as usize;
                            let (a, b) = (words[index], words[index + 1]);
                            (b >= a && a > 0).then(|| PassTime {
                                name: (*name).into(),
                                ms: (b - a) as f32 * period / 1_000_000.,
                            })
                        })
                        .collect();
                    // On tile GPUs the next pass vertex stage can overlap the
                    // previous fragments. Intervals include that waiting; their
                    // sum is NOT GPU frame time. Report their enclosing span.
                    let valid: Vec<_> = slots
                        .iter()
                        .filter(|i| **i != u32::MAX)
                        .map(|i| *i as usize)
                        .collect();
                    let first = valid.iter().map(|i| words[*i]).min().unwrap_or(0);
                    let last = valid.iter().map(|i| words[*i + 1]).max().unwrap_or(0);
                    let span = (first > 0 && last >= first)
                        .then(|| (last - first) as f32 * period / 1_000_000.);
                    drop(bytes);
                    buffer.unmap();
                    if let Ok(mut stored) = result.lock() {
                        *stored = (values, span, sample_id);
                    }
                }
                busy.store(false, Ordering::Relaxed);
            });
    }
    pub fn sample_id(&self) -> u32 {
        self.result.lock().map(|r| r.2).unwrap_or(0)
    }
    pub fn span_ms(&self) -> Option<f32> {
        self.result.lock().ok().and_then(|r| r.1)
    }
    pub fn latest(&self) -> Vec<PassTime> {
        self.result.lock().map(|r| r.0.clone()).unwrap_or_default()
    }
}
