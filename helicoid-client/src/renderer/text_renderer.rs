/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */

use rkyv::{Archive, Deserialize, Serialize};

use log::trace;
use std::fmt::{self, Debug};
use std::sync::Arc;
use winit::dpi::PhysicalSize;

use half::f16;
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};

use crate::{
    editor::{Colors, Style, UnderlineStyle},
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
