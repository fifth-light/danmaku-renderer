use std::{mem::size_of, num::NonZeroUsize};

use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Buffer, BufferUsages, Device, Queue,
};

use crate::danmaku::DanmakuTime;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Pod, Zeroable)]
pub struct TimestampUniform {
    time_millis: u32,
}

impl From<DanmakuTime> for TimestampUniform {
    fn from(value: DanmakuTime) -> Self {
        Self {
            time_millis: value.as_millis(),
        }
    }
}

impl TimestampUniform {
    pub fn prepare(&self, device: &Device) -> Buffer {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Timestamp Buffer"),
            contents: bytemuck::cast_slice(&[*self]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        buffer
    }

    pub fn update(&self, buffer: &Buffer, queue: &Queue) {
        let size: NonZeroUsize = size_of::<TimestampUniform>().try_into().unwrap();
        let mut buffer = queue
            .write_buffer_with(buffer, 0, size.try_into().unwrap())
            .unwrap();
        buffer.copy_from_slice(bytemuck::cast_slice(&[*self]));
    }
}
