use std::{collections::HashMap, sync::Arc};

use crate::{
    block_manager::{Block, BlockContainer, BlockGfx},
    text::ShapedTextBlock,
};
use bytecheck::CheckBytes;
use num_enum::IntoPrimitive;
use rkyv::{Archive, Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt;

/* Simple painting interface for describing the user interface at the helix
backend and transferring it to the front end in a render agnostic way */
/* There are some special considerations to take into account for ids, see the implementation methods for details */
#[derive(Debug, Hash, Eq, Clone, Copy, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockId(pub u16);

pub type BlockLayer = u8;

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockPath {
    path: SmallVec<[RenderBlockId; 16]>,
}

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

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct RenderBlockLocation {
    //    pub path: RenderBlockPath,
    pub id: RenderBlockId,
    /* Location refers to top left corner of the render block */
    pub location: PointF16,
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
