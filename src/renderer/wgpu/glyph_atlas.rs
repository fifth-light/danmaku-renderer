use cosmic_text::{Placement, SwashImage};
use etagere::{size2, Allocation, BucketedAtlasAllocator};
use wgpu::{Extent3d, ImageCopyTexture, ImageDataLayout, Origin3d, Queue, Texture, TextureAspect};

// TODO: add some recycle
#[allow(unused)]
pub(crate) struct GlyphItem {
    pub(crate) placement: Placement,
    pub(crate) tex_coords: (u32, u32),
    pub(crate) tex_size: (u32, u32),
    allocation: Allocation,
}

impl GlyphItem {
    fn new(
        texture: &Texture,
        queue: &Queue,
        image: &SwashImage,
        allocation: Allocation,
        shadow_width: u32,
    ) -> Self {
        let data = &image.data;
        assert!(allocation.rectangle.width() as u32 >= image.placement.width);
        assert!(allocation.rectangle.height() as u32 >= image.placement.height);
        let allocation_x = allocation.rectangle.min.x;
        let allocation_y = allocation.rectangle.min.y;
        let tex_coords = (allocation_x as u32, allocation_y as u32);
        let tex_size = (
            image.placement.width + shadow_width * 2,
            image.placement.height + shadow_width * 2,
        );
        let width = image.placement.width;
        let height = image.placement.height;
        let allocation_x = allocation.rectangle.min.x as u32;
        let allocation_y = allocation.rectangle.min.y as u32;
        let size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        queue.write_texture(
            ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: Origin3d {
                    x: allocation_x + shadow_width,
                    y: allocation_y + shadow_width,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            data,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(image.placement.width),
                rows_per_image: Some(image.placement.height),
            },
            size,
        );
        let new_placement = Placement {
            left: image.placement.left - shadow_width as i32,
            top: image.placement.top + shadow_width as i32,
            width: image.placement.width + shadow_width * 2,
            height: image.placement.height + shadow_width * 2,
        };
        Self {
            placement: new_placement,
            tex_coords,
            tex_size,
            allocation,
        }
    }
}

pub(crate) struct GlyphLayer {
    allocator: BucketedAtlasAllocator,
}

impl GlyphLayer {
    pub(crate) fn new(texture_size: (u32, u32)) -> Self {
        Self {
            allocator: BucketedAtlasAllocator::new(size2(
                texture_size.0 as i32,
                texture_size.1 as i32,
            )),
        }
    }

    pub(crate) fn grow(&mut self, new_size: (u32, u32)) {
        self.allocator
            .grow(size2(new_size.0 as i32, new_size.1 as i32))
    }

    pub(crate) fn clear(&mut self) {
        self.allocator.clear()
    }

    pub(crate) fn new_item(
        &mut self,
        texture: &Texture,
        queue: &Queue,
        image: &SwashImage,
        shadow_width: u32,
    ) -> Option<GlyphItem> {
        let size = size2(
            (image.placement.width + shadow_width * 2) as i32,
            (image.placement.height + shadow_width * 2) as i32,
        );
        self.allocator
            .allocate(size)
            .map(|allocation| GlyphItem::new(texture, queue, image, allocation, shadow_width))
    }
}
