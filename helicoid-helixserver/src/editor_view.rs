use helicoid_protocol::gfx::{PointF32, PointU32};
use helix_view::Editor;
use ordered_float::OrderedFloat;
use std::hash::Hash;

trait GfxComposibleBlock: Hash + PartialEq {
    fn extent(&self) -> PointU32;
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32);
}
#[derive(Hash, PartialEq, Clone)]
struct SizeScale {
    line_height: OrderedFloat<f32>,
}
/* Top at the moment is not in use */
#[derive(Hash, PartialEq)]
struct EditorTop {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct Statusline {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct LeftGutter {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightGutter {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopOverlay {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct BottomOverlay {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct LeftOverlay {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightOverlay {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopRightOverlay {
    extent: ShadowMetaBlock,
    scale: SizeScale,
}

#[derive(Default, Hash, PartialEq)]
struct EditorTextArea {
    extent: PointU32,
}

struct EditorModel {
    scale: SizeScale, // Size of a line, in native pixels
    extent: PointU32, // In native pixels, whatever that is
    view_id: usize,
}
struct EditorContainer {
    top: EditorTop,
    bottom: Statusline,
    left: LeftGutter,
    right: RightGutter, // Scrollbar, minimap etc.
    top_overlay: TopOverlay,
    bottom_overlay: BottomOverlay,
    left_overlay: LeftOverlay,
    right_overlay: RightOverlay,
    topright_overlay: TopRightOverlay,
    center_text: EditorTextArea,
    model: EditorModel,
}

impl From<SizeScale> for f32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height)
    }
}
impl From<SizeScale> for u32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height) as u32
    }
}

impl SizeScale {
    fn round_up(&self) -> u32 {
        self.line_height.ceil() as u32
    }
    fn round_down(&self) -> u32 {
        self.line_height.floor() as u32
    }
}

impl EditorContainer {
    pub fn new(line_height: f32) -> Self {
        Self {
            model: EditorModel {
                scale: SizeScale {
                    line_height: OrderedFloat(line_height),
                },
                extent: PointU32::default(),
                view_id: 0,
            },
            top: EditorTop {},
            bottom: Statusline {},
            left: LeftGutter {},
            right: RightGutter {},
            top_overlay: TopOverlay {},
            bottom_overlay: BottomOverlay {},
            left_overlay: LeftOverlay {},
            right_overlay: RightOverlay {},
            topright_overlay: TopRightOverlay {},
            center_text: EditorTextArea::default(),
        }
    }

    pub fn set_size(&mut self, extent: PointU32) {
        self.model.extent = extent;
        self.lay_out();
    }

    pub fn lay_out(&mut self) {
        /* Updates layout sizes of the different elements */
        self.top.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.bottom.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.left.set_layout(
            self.model.scale.clone(),
            PointU32::new(0, self.model.extent.y()),
        );
        self.right.set_layout(
            self.model.scale.clone(),
            PointU32::new(0, self.model.extent.y()),
        );
        let horizontal_left = self
            .model
            .extent
            .y()
            .saturating_sub(self.top.extent().y())
            .saturating_sub(self.bottom.extent().y());
        let vertical_left = self
            .model
            .extent
            .x()
            .saturating_sub(self.right.extent().x())
            .saturating_sub(self.left.extent().x());
        self.center_text.set_layout(
            self.model.scale.clone(),
            PointU32::new(horizontal_left, vertical_left),
        );
    }
}

impl GfxComposibleBlock for EditorTop {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        //        self.extent()
    }
}
impl GfxComposibleBlock for Statusline {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for LeftGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for RightGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for TopOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for BottomOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for LeftOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for RightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for TopRightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for EditorTextArea {
    fn extent(&self) -> PointU32 {
        self.extent
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.extent = extent;
    }
}
