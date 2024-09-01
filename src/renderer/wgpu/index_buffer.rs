use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Buffer, BufferAddress, BufferSlice, BufferUsages, Device,
};

#[derive(Default)]
pub(crate) struct IndexBuffer {
    cache: Vec<u32>,
    buffer: Option<Buffer>,
}

impl IndexBuffer {
    pub(crate) fn ensure_size(&mut self, device: &Device, glyphs: u32) {
        if glyphs == 0 {
            return;
        }
        assert!(self.cache.len() % 6 == 0);
        let cached_glyphs = self.cache.len() as u32 / 6;
        if cached_glyphs >= glyphs {
            return;
        }

        let start = cached_glyphs;
        let count = glyphs - cached_glyphs;
        self.cache.reserve(count as usize * 6);
        (start..start + count)
            .flat_map(|index| {
                let start = index * 4;
                [start, start + 2, start + 1, start + 1, start + 2, start + 3]
            })
            .for_each(|item| self.cache.push(item));
        assert_eq!(self.cache.len(), glyphs as usize * 6);

        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Indices buffer (size: {})", glyphs)),
            contents: bytemuck::cast_slice(&self.cache),
            usage: BufferUsages::INDEX,
        });
        self.buffer = Some(buffer)
    }

    pub(crate) fn buffer_slice(&self, glyphs: u32) -> BufferSlice {
        assert!(self.cache.len() >= glyphs as usize * 6);
        let buffer = self.buffer.as_ref().expect("Buffer is empty");
        buffer.slice(0..(glyphs as BufferAddress * 6 * 4))
    }
}
