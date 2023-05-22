use std::hash::Hash;

use crate::{
    block_manager::{Block, BlockContainer, BlockGfx},
    text::ShapedTextBlock,
};
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;
use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt;

pub const SVG_RESOURCE_NAME_LEN: usize = 32;

/* Simple painting interface for describing the user interface at the helix
backend and transferring it to the front end in a render agnostic way */
/* There are some special considerations to take into account for ids, see the implementation methods for details */
#[derive(
    Debug,
    Hash,
    Eq,
    Clone,
    Copy,
    PartialEq,
    Archive,
    Serialize,
    Deserialize,
    CheckBytes,
    Ord,
    PartialOrd,
)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockId(pub u16);

pub type BlockLayer = u8;

#[derive(
    Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes, Ord, PartialOrd,
)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockPath {
    path: SmallVec<[RenderBlockId; 16]>,
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum SimpleLineStyle {
    Rounded,
    #[default]
    Angled,
    None,
}

#[derive(
    Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum SimpleBlendMode {
    Clear,
    Src,
    Dst,
    #[default]
    SrcOver,
    DstOver,
    SrcIn,
    DstIn,
    SrcOut,
    DstOut,
    SrcATop,
    DstATop,
    Xor,
    Plus,
    Modulate,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Multiply,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimplePaint {
    pub line_color: u32,
    pub fill_color: u32,
    line_width: u16,             // half float
    background_blur_amount: u16, // half float
    pub line_style: SimpleLineStyle,
    pub blend: SimpleBlendMode,
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct FontPaint {
    pub color: u32,
    pub blend: SimpleBlendMode,
}

#[derive(Hash, Eq, Copy, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct PointF16 {
    x: u16, // half float
    y: u16, // half float
}

#[derive(
    Hash, Copy, Clone, PartialEq, Eq, Default, Debug, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct PointF32 {
    x: OrderedFloat<f32>,
    y: OrderedFloat<f32>,
}

#[derive(
    Copy, Clone, Hash, PartialEq, Debug, Default, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct PointU32 {
    x: u32,
    y: u32,
}

#[derive(
    Copy, Clone, PartialEq, Debug, Archive, Serialize, Deserialize, CheckBytes, Default, Hash,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct PointU16 {
    x: u16,
    y: u16,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, IntoPrimitive)]
#[archive_attr(derive(Debug))]
#[repr(u8)]
pub enum PathVerb {
    Move,
    Line,
    Quad,
    Conic,
    Cubic,
    Close,
    Done,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct SimpleDrawPath {
    pub paint: SimplePaint,
    pub draw_elements: SmallVec<[(PathVerb, PointF16, PointF16, PointF16); 16]>,
}

/// Shorthand for draw path for simple polygons: The first element is move,
/// the rest are points on the polygon. The polygon is then closed.
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleDrawPolygon {
    pub paint: SimplePaint,
    pub draw_elements: SmallVec<[PointF16; 16]>,
    pub closed: bool,
}
/// This element just fill the whole surface with the paint
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleFill {
    pub paint: SimplePaint,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleRoundRect {
    pub paint: SimplePaint,
    pub topleft: PointF16,
    pub bottomright: PointF16,
    pub roundedness: PointF16,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleSvg {
    pub resource_name: SmallVec<[u8; SVG_RESOURCE_NAME_LEN]>,
    pub location: PointF32,
    pub extent: PointF32,
    pub paint: SimplePaint,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum SimpleDrawElement {
    Path(SimpleDrawPath),
    Polygon(SimpleDrawPolygon),
    Fill(SimpleFill),
    RoundRect(SimpleRoundRect),
    SvgResource(SimpleSvg),
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SimpleDrawBlock {
    pub extent: PointF32,
    pub draw_elements: SmallVec<[SimpleDrawElement; 32]>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct MetaDrawBlock {
    pub extent: PointF32,
    pub buffered: bool,
    pub alpha: Option<u8>, // If alpha is 0, the block is skipped, otherwise only applies to buffered blocks
    pub sub_blocks: SmallVec<[RenderBlockLocation; 32]>,
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
    pub update: bool,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockLocation {
    //    pub path: RenderBlockPath,
    pub id: RenderBlockId,
    /* Location refers to top left corner of the render block */
    pub location: PointF32,
    pub layer: u8, /* Render order/layer, 0 is rendered first (bottommost).
                   Blocks with same number can be rendered in any order */
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockRemoveInstruction {
    pub offset: RenderBlockId,
    pub mask: RenderBlockId,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RemoteBoxUpdate {
    pub parent: RenderBlockPath,
    pub new_render_blocks: SmallVec<[NewRenderBlock; 4]>,
    pub remove_render_blocks: SmallVec<[RenderBlockRemoveInstruction; 4]>,
    pub move_block_locations: SmallVec<[RenderBlockLocation; 128]>,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub enum RemoteSingleChangeElement {
    NewRenderBlocks(SmallVec<[NewRenderBlock; 4]>),
    RemoveRenderBlocks(SmallVec<[RenderBlockRemoveInstruction; 4]>),
    MoveBlockLocations(SmallVec<[RenderBlockLocation; 32]>),
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RemoteSingleChange {
    pub parent: RenderBlockPath,
    pub change: RemoteSingleChangeElement,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct HelicoidToClientMessage {
    pub updates: Vec<RemoteSingleChange>,
}

impl SimplePaint {
    pub fn new(line_color: Option<u32>, fill_color: Option<u32>, line_width: Option<f32>) -> Self {
        Self {
            line_color: line_color.unwrap_or(0),
            fill_color: fill_color.unwrap_or(0),
            line_width: half::f16::from_f32(line_width.unwrap_or(0f32)).to_bits(),
            line_style: SimpleLineStyle::None,
            background_blur_amount: half::f16::from_f32(0f32).to_bits(),
            blend: SimpleBlendMode::SrcOver,
        }
    }
    pub fn set_line_width(&mut self, line_width: f32) {
        self.line_width = half::f16::from_f32(line_width).to_bits();
    }
    pub fn line_width(&self) -> f32 {
        half::f16::from_bits(self.line_width).to_f32()
    }
    pub fn set_background_blur_amount(&mut self, background_blur_amount: f32) {
        self.background_blur_amount = half::f16::from_f32(background_blur_amount).to_bits();
    }
    pub fn background_blur_amount(&self) -> f32 {
        half::f16::from_bits(self.background_blur_amount).to_f32()
    }
}
impl SimpleDrawElement {
    pub fn fill(paint: SimplePaint) -> Self {
        Self::Fill(SimpleFill { paint })
    }
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
impl PointF32 {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: OrderedFloat(x),
            y: OrderedFloat(y),
        }
    }
    pub fn x(&self) -> f32 {
        f32::from(self.x)
    }
    pub fn y(&self) -> f32 {
        f32::from(self.y)
    }
}
impl From<PointF32> for PointF16 {
    fn from(value: PointF32) -> Self {
        PointF16::new(value.x(), value.y())
    }
}

impl From<PointF16> for PointF32 {
    fn from(value: PointF16) -> Self {
        PointF32::new(value.x(), value.y())
    }
}

impl PointU32 {
    pub fn floor<P: Into<PointF32>>(p: P) -> Self {
        let p32: PointF32 = p.into();
        Self {
            x: p32.x.floor() as u32,
            y: p32.y.floor() as u32,
        }
    }
    pub fn ceil<P: Into<PointF32>>(p: P) -> Self {
        let p32: PointF32 = p.into();
        Self {
            x: p32.x.ceil() as u32,
            y: p32.y.ceil() as u32,
        }
    }
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
    pub fn x(&self) -> u32 {
        self.x
    }
    pub fn y(&self) -> u32 {
        self.y
    }
}

impl From<PointU32> for PointF16 {
    fn from(value: PointU32) -> Self {
        PointF16::new(value.x() as f32, value.y() as f32)
    }
}

impl PointU16 {
    pub fn floor<P: Into<PointF32>>(p: P) -> Self {
        let p32: PointF32 = p.into();
        Self {
            x: p32.x.floor() as u16,
            y: p32.y.floor() as u16,
        }
    }
    pub fn ceil<P: Into<PointF32>>(p: P) -> Self {
        let p32: PointF32 = p.into();
        Self {
            x: p32.x.ceil() as u16,
            y: p32.y.ceil() as u16,
        }
    }
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
    pub fn x(&self) -> u16 {
        self.x
    }
    pub fn y(&self) -> u16 {
        self.y
    }
}

impl RenderBlockPath {
    pub fn top() -> Self {
        Self {
            path: smallvec::smallvec![],
        }
    }
    pub fn new(path: SmallVec<[RenderBlockId; 16]>) -> Self {
        Self { path }
    }
    pub fn child(parent: &RenderBlockPath, child_id: RenderBlockId) -> Self {
        let mut path = parent.path.clone();
        path.push(child_id);
        Self { path }
    }
    pub fn path(&self) -> &SmallVec<[RenderBlockId; 16]> {
        &self.path
    }

    pub fn is_relative(&self) -> bool {
        if !self.path.is_empty() {
            return !self.path[0].is_relative_id();
        } else {
            false
        }
    }

    pub fn common_start(&self, other: &Self) -> bool {
        let common_length = self.path.len().min(other.path.len());
        self.path[0..common_length] == other.path[0..common_length]
    }

    pub fn resolved<C: BlockContainer<G>, G: BlockGfx>(
        &self,
        container: &C,
    ) -> anyhow::Result<Self> {
        if !self.is_relative() {
            // Check that the start of this path matches the start of the container
            if !self.common_start(container.path()) {
                /* If the start is not common this is not a compatible path to
                   resolve, so return an error. In the future make a proper error
                type so the callers of this function get structured error information
                that can be acted upon */
                return Err(anyhow::anyhow!("Incompatible path"));
            } else {
                Ok(self.clone())
            }
        } else {
            /* Make this path absolute */
            /* First concatenate the root and the relative path, then call a
            function that removes relativeness */
            container.path().concatenated(self).remove_parent_segments()
        }
    }

    /** Resolves / gets a reference to the block at a given path
     * this internal variant of the function is used to traverse the path
     * (using idx) recursively */
    fn resolve_block_mut_internal<'b, G: BlockGfx>(
        &self,
        block: &'b mut Block<G>,
        idx: usize,
    ) -> Option<&'b mut Block<G>> {
        if let Some(container) = block.meta_mut().as_container_mut() {
            log::trace!("Resolve internal: {:?} ({})", container.path(), idx);
            let id = self.path[idx];
            let child = container.block_mut(id);
            if idx == self.path.len() - 1 {
                child
            } else {
                if let Some(child) = child {
                    self.resolve_block_mut_internal(child, idx + 1)
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    /** Resolves / gets a reference to the block at a given path */
    pub fn resolve_block_mut<'b, G: BlockGfx>(
        &self,
        block: &'b mut Block<G>,
    ) -> Option<&'b mut Block<G>> {
        if block.meta_mut().as_container_mut().is_some() && !self.path.is_empty() {
            self.resolve_block_mut_internal(block, 0)
        } else {
            log::trace!(
                "Trying to resolve in empty container or empty path: {:?} / {:?}",
                block.meta_mut().as_container_mut(),
                self.path
            );
            None
        }
    }

    pub fn concatenated(&self, other: &RenderBlockPath) -> RenderBlockPath {
        let mut new_vec = self.path.clone();
        new_vec.extend_from_slice(other.path.as_slice());
        Self { path: new_vec }
    }

    fn remove_parent_segments(&self) -> anyhow::Result<Self> {
        let mut new_path = self.clone();
        let mut i = 0;
        while i < new_path.path.len() {
            if new_path.path[i].is_parent_id() {
                if i < 1 {
                    /* Try to get parent when at top, bail */
                    return Err(anyhow::anyhow!(
                        "Invalid parent at root for path: {:?}",
                        self
                    ));
                }
                /* Remove current element, and element with index before */
                new_path.path.remove(i - 1);
                new_path.path.remove(i - 1);
                i -= 1;
            } else {
                i += 1;
            }
        }
        Ok(new_path)
    }
}

/* TODO: Retrieve this from the refactored helicone code */

impl RenderBlockId {
    const PARENT_ID: u16 = 0xFFFF;
    const RELATIVE_ID: u16 = 0xFFFE; // Not sure how to use relative id's yet
    const RESERVED_MIN_ID: u16 = 0xFFF0;
    pub fn normal(value: u16) -> anyhow::Result<Self> {
        if value >= Self::RESERVED_MIN_ID {
            Err(anyhow::anyhow!(
                "Tried to contruct a reserved id value as a normal value {}",
                value
            ))
        } else {
            Ok(Self(value))
        }
    }
    pub fn parent() -> Self {
        Self(Self::PARENT_ID)
    }
    pub fn relative() -> Self {
        Self(Self::RELATIVE_ID)
    }
    pub fn from_wire(value: u16) -> Self {
        Self(value)
    }
    pub fn is_parent_id(&self) -> bool {
        self.0 == Self::PARENT_ID
    }
    pub fn is_relative_id(&self) -> bool {
        self.0 == Self::RELATIVE_ID
    }
}
