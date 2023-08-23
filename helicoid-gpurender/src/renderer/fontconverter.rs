use helicoid_protocol::text::{
    ShapedStringMetadataCoordinates, ShapedStringMetadataCoordinatesResolver,
    ShapedStringMetadataSpan, ShapedTextBlock,
};
use wgpu::Origin2d;

use crate::font::fontcache::{Fixed88, PackedSubpixels, RenderSpec, RenderSpecElement};

pub struct FontConverter {
    pub temp_spec: RenderSpec,
    //    pub temp_pans: Vec<ShapedStringMetadataSpan>,
}

impl FontConverter {
    pub fn convert_and_set(
        &mut self,
        text: &ShapedTextBlock,
        run_idx: usize,
    ) -> Option<&RenderSpec> {
        self.temp_spec.clear();
        if text.metadata.runs.len() >= run_idx {
            return None;
        }
        let run = text.metadata.runs.get(run_idx).unwrap();
        let key_font_size = Fixed88::from(f32::from(run.font_info.font_parameters.size));
        let mut span_start;
        let mut span_end = 0;
        let default_coordinates = ShapedStringMetadataCoordinates::default();
        for span in text.metadata.spans.iter() {
            if span.metadata_info != run_idx as u8 {
                continue;
            }
            span_start = span_end;
            span_end = span_start + span.substring_length;
            let coordinates = text
                .metadata
                .span_coordinates
                .get(span.span_coordinates as usize)
                .unwrap_or(&default_coordinates);

            for glyph in text.glyphs[span_start as usize..span_end as usize].iter() {
                let render_element = RenderSpecElement {
                    color_idx: 0, //todo: Figure out how to look up color from paint
                    key_glyph_id: glyph.glyph(),
                    key_font_size,
                    key_bins: PackedSubpixels::default(),
                    offset: Origin2d {
                        x: f32::from(coordinates.baseline_x) as u32 + glyph.x() as u32,
                        y: f32::from(coordinates.baseline_y) as u32 + glyph.y() as u32,
                    },
                    extent: (10, 10), //(0, 0), // todo: Can we make extent somehow?
                };
                self.temp_spec.add_element(render_element);
            }
        }
        Some(&self.temp_spec)
    }
}
