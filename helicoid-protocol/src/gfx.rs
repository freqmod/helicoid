use std::{collections::HashMap, sync::Arc};

use crate::text::ShapedTextBlock;
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use parking_lot::Mutex;
use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt;

/* Simple painting interface for describing the user interface at the helix
backend and transferring it to the front end in a render agnostic way */
pub type RenderBlockId = u16;

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum SimpleLineStyle {
    None,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimplePaint {
    line_color: u32,
    fill_color: u32,
    line_width: u16, // half float
    line_style: SimpleLineStyle,
}
#[derive(Hash, Eq, Copy, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct PointF16 {
    x: u16, // half float
    y: u16, // half float
}
/*
/// To be implemented
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[derive(IntoPrimitive)]
#[repr(u8)]
enum PathVerb {
    Move,
    Line,
    Quad,
    Conic,
    Cubic,
    Close,
    Done,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
struct SimpleDrawPath {
    paint: SimplePaint,
    draw_elements: SmallVec<[(PathVerb, PointF16, PointF16, PointF16); 16]>,
}
*/

/// Shorthand for draw path for simple polygons: The first element is move,
/// the rest are points on the polygon. The polygon is then closed.
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleDrawPolygon {
    pub paint: SimplePaint,
    pub draw_elements: SmallVec<[PointF16; 16]>,
}
/// This element just fill the whole surface with the paint
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleFill {
    pub paint: SimplePaint,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum SimpleDrawElement {
    //    Path(SimpleDrawPath),
    Polygon(SimpleDrawPolygon),
    Fill(SimpleFill),
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleDrawBlock {
    pub extent: PointF16,
    pub draw_elements: SmallVec<[SimpleDrawElement; 32]>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct MetaDrawBlock {
    pub extent: PointF16,
    pub sub_blocks: SmallVec<[RenderBlockLocation; 64]>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum RenderBlockDescription {
    ShapedTextBlock(ShapedTextBlock),
    SimpleDraw(SimpleDrawBlock),
    MetaBox(MetaDrawBlock),
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct NewRenderBlock {
    pub id: RenderBlockId,
    pub contents: RenderBlockDescription,
}

#[derive(Debug, Hash, Eq, Clone, Copy, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockLocation {
    pub id: RenderBlockId,
    /* Location refers to top left corner of the render block */
    pub location: PointF16,
    pub layer: u8, /* Render order/layer, 0 is rendered first (bottommost).
                   Blocks with same number can be rendered in any order */
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RemoteBoxUpdate {
    pub new_render_blocks: SmallVec<[NewRenderBlock; 4]>,
    pub remove_render_blocks: SmallVec<[RenderBlockId; 4]>,
    pub render_block_locations: SmallVec<[RenderBlockLocation; 128]>,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct HelicoidToClientMessage {
    pub update: RemoteBoxUpdate,
}

impl PointF16 {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: half::f16::from_f32(x).to_bits(),
            y: half::f16::from_f32(y).to_bits(),
        }
    }
    pub fn x(&self) -> f32 {
        half::f16::from_bits(self.x).to_f32()
    }
    pub fn y(&self) -> f32 {
        half::f16::from_bits(self.y).to_f32()
    }
}
impl Default for PointF16 {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}
impl fmt::Debug for PointF16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointF16")
            .field("x", &self.x())
            .field("y", &self.y())
            .finish()
    }
}
