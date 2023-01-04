use std::{collections::HashMap, sync::Arc};

use crate::text::ShapedTextBlock;
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use parking_lot::Mutex;
use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;

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
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
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
    paint: SimplePaint,
    draw_elements: SmallVec<[PointF16; 16]>,
}
/// This element just fill the whole surface with the paint
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleFill {
    paint: SimplePaint,
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
    pub width: u16,
    pub height: u16,
    draw_elements: SmallVec<[SimpleDrawElement; 32]>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum RenderBlockDescription {
    ShapedTextBlock(ShapedTextBlock),
    SimpleDraw(SimpleDrawBlock),
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct NewRenderBlock {
    id: RenderBlockId,
    contents: RenderBlockDescription,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockLocation {
    id: RenderBlockId,
    /* Location refers to top left corner of the render block */
    location_x: u16,
    location_y: u16,
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
    update: RemoteBoxUpdate,
}
