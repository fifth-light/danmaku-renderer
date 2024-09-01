use std::{mem::size_of, num::NonZeroUsize};

use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Buffer, BufferUsages, Device, Queue,
};

use crate::worker::DanmakuParam;

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ConfigUniform {
    screen_width: u32,
    screen_height: u32,
    line_height: u32,
    lifetime: u32,
}

impl From<DanmakuParam> for ConfigUniform {
    fn from(value: DanmakuParam) -> Self {
        ConfigUniform {
            screen_width: value.screen_size.0,
            screen_height: value.screen_size.1,
            line_height: value.line_height,
            lifetime: value.lifetime.as_millis() as u32,
        }
    }
}

impl ConfigUniform {
    pub fn prepare(&self, device: &Device) -> Buffer {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Config Buffer"),
            contents: bytemuck::cast_slice(&[*self]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        buffer
    }

    pub fn update(&self, buffer: &Buffer, queue: &Queue) {
        let size: NonZeroUsize = size_of::<ConfigUniform>().try_into().unwrap();
        let mut buffer = queue
            .write_buffer_with(buffer, 0, size.try_into().unwrap())
            .unwrap();
        buffer.copy_from_slice(bytemuck::cast_slice(&[*self]));
    }
}
