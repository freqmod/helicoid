/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */





use std::sync::Arc;






use crate::{
    editor::{Style},
    //dimensions::Dimensions,
    renderer::CachingShaper,
};

pub struct TextRenderer {
    pub shaper: CachingShaper,
    //pub paint: Paint,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    //pub font_dimensions: Dimensions,
    pub scale_factor: f64,
    pub is_ready: bool,
}
