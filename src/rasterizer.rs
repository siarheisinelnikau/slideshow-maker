use egui_software_backend::{ColorFieldOrder, EguiSoftwareRender};

#[derive(Default)]
pub struct SceneCache {
    shapes: Vec<egui::epaint::ClippedShape>,
    size: (u32, u32),
    scale: f32,
}
impl SceneCache {
    pub fn changed(
        &mut self,
        shapes: &[egui::epaint::ClippedShape],
        size: (u32, u32),
        scale: f32,
        textures_changed: bool,
    ) -> bool {
        if !textures_changed && self.size == size && self.scale == scale && self.shapes == shapes {
            return false;
        }
        self.shapes = shapes.to_vec();
        self.size = size;
        self.scale = scale;
        true
    }
}

pub fn create() -> EguiSoftwareRender {
    // The upstream rectangle fast path can overrun a photo texture at near-1:1
    // fractional scaling. Keep caching/raster optimizations, but use the safe
    // bilinear triangle path instead of converting paired triangles to rectangles.
    EguiSoftwareRender::new(ColorFieldOrder::Bgra).with_convert_tris_to_rects(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_photo_scaling_stays_inside_texture() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "edge fixture",
            egui::ColorImage::filled([640, 360], egui::Color32::from_rgb(30, 170, 90)),
            egui::TextureOptions::LINEAR,
        );
        let output = ctx.run(egui::RawInput::default(), |_| {});
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(641.0, 361.0));
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.add_rect_with_uv(
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let primitives = [egui::ClippedPrimitive {
            clip_rect: rect,
            primitive: egui::epaint::Primitive::Mesh(mesh),
        }];
        let mut pixels = vec![0u32; 641 * 361];
        create().render(
            &mut egui_software_backend::BufferMutRef::new(
                bytemuck::cast_slice_mut(&mut pixels),
                641,
                361,
            ),
            &primitives,
            &output.textures_delta,
            1.0,
        );
        assert_eq!(pixels[180 * 641 + 320] & 0x00ff_ffff, 0x001e_aa5a);
        assert_eq!(pixels[360 * 641 + 640] & 0x00ff_ffff, 0x001e_aa5a);
    }
}
