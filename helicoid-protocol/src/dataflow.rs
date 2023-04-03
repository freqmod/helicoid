use ahash::AHasher;
use hashbrown::HashSet;
use smallvec::smallvec;
use std::{
    any::Any,
    hash::{Hash, Hasher},
    ops::Deref,
};

use crate::{
    gfx::{
        MetaDrawBlock, NewRenderBlock, PointF16, RemoteBoxUpdate, RenderBlockDescription,
        RenderBlockId, RenderBlockLocation, RenderBlockPath, SimpleDrawBlock,
    },
    text::ShapedTextBlock,
};
trait Observer<T>
where
    T: PartialEq + Hash + Clone,
{
    fn data_changed(&mut self, data: &ObservableState<T>);
}
pub struct ObservableReference<T>
where
    T: PartialEq + Hash + Clone,
{
    value: T,
    hash: u64,
}
pub struct ObservableState<T>
where
    T: PartialEq + Hash + Clone,
{
    current: T,
    reference: Option<ObservableReference<T>>,
    observers: HashSet<Box<dyn Observer<T>>>,
}

impl<T> ObservableState<T>
where
    T: PartialEq + Hash + Clone,
{
    pub fn new(state: T) -> Self {
        Self {
            current: state,
            reference: None,
            observers: Default::default(),
        }
    }
    pub fn check_changed(&mut self) {
        let mut hasher = AHasher::default();
        self.current.hash(&mut hasher);
        let new_hash = hasher.finish();
        if let Some(reference) = self.reference.as_ref() {
            if reference.hash != new_hash {
                if reference.value != self.current {
                    self.fire_changed();
                }
            }
        }
    }
    fn fire_changed(&mut self) {}
    fn subscribe(&mut self, observer: Box<dyn Observer<T>>) {
        //        self.observer.insert(observer);
    }
    fn unsubscribe(&mut self, observer: Box<dyn Observer<T>>) {
        //        self.observers.insert(observer);
    }
}

struct ObservableGuard<'a, T>
where
    T: PartialEq + Hash + Clone,
{
    state: &'a mut ObservableState<T>,
}

impl<'a, T> Drop for ObservableGuard<'a, T>
where
    T: PartialEq + Hash + Clone,
{
    fn drop(&mut self) {
        self.state.check_changed();
    }
}
/* Type erased Container (inspired by Xilem) */
pub trait AnyShadowMetaContainerBlock: Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock) -> bool;
    fn hash_value(&self) -> u64;
}

pub trait ContainerBlockLogic: Send + Hash + PartialEq {
    fn pre_update(block: &mut ShadowMetaContainerBlock<Self>)
    where
        Self: Sized;
    fn post_update(block: &mut ShadowMetaContainerBlock<Self>)
    where
        Self: Sized;
}
/* Container block logic, without any logic, used as a filler when setting up a
container that has no real logic associated */
#[derive(Default, Hash, PartialEq)]
pub struct NoContainerBlockLogic {}
impl ContainerBlockLogic for NoContainerBlockLogic {
    fn pre_update(block: &mut ShadowMetaContainerBlock<Self>)
    where
        Self: Sized,
    {
    }

    fn post_update(block: &mut ShadowMetaContainerBlock<Self>)
    where
        Self: Sized,
    {
    }
}
pub enum ShadowMetaBlock {
    WrappedContainer(Box<dyn AnyShadowMetaContainerBlock>),
    Container(ShadowMetaContainerBlock<NoContainerBlockLogic>),
    Draw(ShadowMetaDrawBlock),
    Text(ShadowMetaTextBlock),
}

impl PartialEq for ShadowMetaBlock {
    fn eq(&self, other: &Self) -> bool {
        match self {
            ShadowMetaBlock::WrappedContainer(wc) => {
                if let ShadowMetaBlock::WrappedContainer(other) = other {
                    wc.eq(other.as_ref())
                } else {
                    false
                }
            }
            ShadowMetaBlock::Container(c) => {
                if let ShadowMetaBlock::Container(other) = other {
                    PartialEq::eq(c, other)
                } else {
                    false
                }
            }
            ShadowMetaBlock::Draw(d) => {
                if let ShadowMetaBlock::Draw(other) = other {
                    d.eq(other)
                } else {
                    false
                }
            }
            ShadowMetaBlock::Text(t) => {
                if let ShadowMetaBlock::Text(other) = other {
                    t.eq(other)
                } else {
                    false
                }
            }
        }
    }
}

impl<L> AnyShadowMetaContainerBlock for ShadowMetaContainerBlock<L>
where
    L: ContainerBlockLogic + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }

    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock) -> bool {
        if let Some(rhs) = rhs.as_any().downcast_ref::<Self>() {
            PartialEq::eq(self, rhs)
        } else {
            false
        }
    }

    fn hash_value(&self) -> u64 {
        let mut hasher = AHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Hash for ShadowMetaBlock {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ShadowMetaBlock::WrappedContainer(wc) => {
                state.write_u64(wc.hash_value());
            }
            ShadowMetaBlock::Container(c) => {
                c.hash(state);
            }
            ShadowMetaBlock::Draw(d) => {
                d.hash(state);
            }
            ShadowMetaBlock::Text(t) => {
                t.hash(state);
            }
        }
    }
}

#[derive(Hash, PartialEq)]
pub struct ShadowMetaContainerBlockInner {
    id: RenderBlockId,
    wire: MetaDrawBlock,
    child_blocks: Vec<ShadowMetaBlock>, // Corresponding index wise to the sub_blocks in wire
    hash: Option<u64>,
    client_hash: Option<u64>,
    meta_hash: u64,
    client_meta_hash: Option<u64>,
}

#[derive(Hash, PartialEq)]
pub struct ShadowMetaContainerBlock<L>
where
    L: ContainerBlockLogic,
{
    inner: ShadowMetaContainerBlockInner,
    logic: L,
}

#[derive(Hash, PartialEq)]
pub struct WrappedShadowMetaContainerBlock {}

#[derive(Hash, PartialEq)]
pub struct ShadowMetaDrawBlock {
    pub wire: SimpleDrawBlock,
    hash: Option<u64>,
    client_hash: Option<u64>,
}

#[derive(Hash, PartialEq)]
pub struct ShadowMetaTextBlock {
    pub wire: ShapedTextBlock,
    hash: Option<u64>,
    client_hash: Option<u64>,
}

impl<L> ShadowMetaContainerBlock<L>
where
    L: ContainerBlockLogic,
{
    pub fn new(
        id: RenderBlockId,
        extent: PointF16,
        buffered: bool,
        alpha: Option<u8>,
        logic: L,
    ) -> Self {
        let mut s = Self {
            inner: ShadowMetaContainerBlockInner {
                id,
                wire: MetaDrawBlock {
                    extent,
                    buffered,
                    alpha,
                    sub_blocks: Default::default(),
                },
                child_blocks: Default::default(),
                hash: None,
                client_hash: None,
                meta_hash: 0,
                client_meta_hash: None,
            },
            logic,
        };
        s.inner.rehash();
        s
    }
    pub fn inner_ref(&self) -> &ShadowMetaContainerBlockInner {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut ShadowMetaContainerBlockInner {
        &mut self.inner
    }
    pub fn logic_ref(&self) -> &L {
        &self.logic
    }
    pub fn destruct_mut(&mut self) -> (&mut ShadowMetaContainerBlockInner, &mut L) {
        let Self { inner, logic } = self;
        (inner, logic)
    }
    pub fn extent(&self) -> PointF16 {
        self.inner.wire.extent
    }
    pub fn set_extent(&mut self, extent: PointF16) {
        self.inner.wire.extent = extent;
    }
    pub fn alpha(&mut self) -> Option<u8> {
        self.inner.wire.alpha
    }
    pub fn set_alpha(&mut self, alpha: Option<u8>) {
        self.inner.wire.alpha = alpha;
    }
    pub fn set_child(&mut self, location: RenderBlockLocation, block: ShadowMetaBlock) {
        self.inner.set_child(location, block)
    }
    pub fn remove_child(
        &mut self,
        id: RenderBlockId,
    ) -> Option<(ShadowMetaBlock, RenderBlockLocation)> {
        self.inner.remove_child(id)
    }
    pub fn child(&self, id: RenderBlockId) -> Option<(&ShadowMetaBlock, &RenderBlockLocation)> {
        self.inner.child(id)
    }
    pub fn child_mut(&mut self, id: RenderBlockId) -> Option<ShadowMetaContainerBlockGuard> {
        self.inner.child_mut(id)
    }
}
impl ShadowMetaContainerBlockInner {
    pub fn extent(&self) -> PointF16 {
        self.wire.extent
    }
    pub fn set_extent(&mut self, extent: PointF16) {
        self.wire.extent = extent;
    }
    pub fn alpha(&mut self) -> Option<u8> {
        self.wire.alpha
    }
    pub fn set_alpha(&mut self, alpha: Option<u8>) {
        self.wire.alpha = alpha;
    }
    pub fn set_child(&mut self, location: RenderBlockLocation, block: ShadowMetaBlock) {
        let idx = if let Some(block_idx) = self
            .wire
            .sub_blocks
            .iter()
            .enumerate()
            .find_map(|(idx, b)| if b.id == location.id { Some(idx) } else { None })
        {
            self.wire.sub_blocks[block_idx] = location;
            self.child_blocks[block_idx] = block;
            block_idx
        } else {
            assert!(self.wire.sub_blocks.len() == self.child_blocks.len());
            self.wire.sub_blocks.push(location);
            self.child_blocks.push(block);
            self.child_blocks.len() - 1
        };
        self.check_changed(idx);
    }
    pub fn remove_child(
        &mut self,
        id: RenderBlockId,
    ) -> Option<(ShadowMetaBlock, RenderBlockLocation)> {
        let res = if let Some(block_idx) = self
            .wire
            .sub_blocks
            .iter()
            .enumerate()
            .find_map(|(idx, b)| if b.id == id { Some(idx) } else { None })
        {
            Some((
                self.child_blocks.remove(block_idx),
                self.wire.sub_blocks.remove(block_idx),
            ))
        } else {
            None
        };
        self.rehash();
        res
    }
    pub fn child(&self, id: RenderBlockId) -> Option<(&ShadowMetaBlock, &RenderBlockLocation)> {
        if let Some((block_idx, block_location)) = self
            .wire
            .sub_blocks
            .iter()
            .enumerate()
            .find_map(|(idx, b)| if b.id == id { Some((idx, b)) } else { None })
        {
            self.child_blocks
                .get(block_idx)
                .map(|block| (block, block_location))
        } else {
            None
        }
    }
    /* NB/Safety: If the id in the location is changed make sure it is not duplicating other id's */
    pub fn child_mut(&mut self, id: RenderBlockId) -> Option<ShadowMetaContainerBlockGuard> {
        if let Some(block_idx) = self
            .wire
            .sub_blocks
            .iter_mut()
            .enumerate()
            .find_map(|(idx, b)| if b.id == id { Some(idx) } else { None })
        {
            Some(ShadowMetaContainerBlockGuard {
                container_inner: self,
                idx: block_idx,
            })
        /*            self.child_blocks
        .get_mut(block_idx)
        .map(|block| (block, block_location))*/
        } else {
            None
        }
    }
    fn check_changed(&mut self, _idx: usize) {
        //We'll just rehash in case, it should be fast enough
        self.rehash();
    }
    fn rehash(&mut self) {
        let mut meta_hasher = AHasher::default();
        self.id.hash(&mut meta_hasher);
        self.wire.hash(&mut meta_hasher);
        self.meta_hash = meta_hasher.finish();

        let mut hasher = AHasher::default();
        hasher.write_u64(self.meta_hash);
        //self.wire.hash(&mut hasher);
        // We need to keep track of the sub block state on the client

        for child in self.child_blocks.iter() {
            if let Some(hash) = match child {
                ShadowMetaBlock::WrappedContainer(wc) => Some(wc.hash_value()),
                ShadowMetaBlock::Container(c) => c.inner.hash, // TODO: We should probably hash the logic value too
                ShadowMetaBlock::Draw(d) => d.hash,
                ShadowMetaBlock::Text(t) => t.hash,
            } {
                hasher.write_u64(hash);
            } else {
                hasher.write_u64(0);
            }
        }
        let new_hash = hasher.finish();
        self.hash = Some(new_hash);
    }
    pub fn client_transfer_messages(
        &mut self,
        parent: &RenderBlockPath, // nb remember to append the id of this box for children
        messages_vec: &mut Vec<RemoteBoxUpdate>,
    ) {
        /* Make messages that transfers all outstanding state to client */
        if self.hash.is_none() {
            self.rehash();
        }
        if self.client_hash.is_some() || self.hash == self.client_hash {
            return;
        }
        let child_path = RenderBlockPath::child(parent, self.id);
        /* TODO: NB: Push any contents that the current box refers to before sending the message about them */
        for element in self.child_blocks.iter_mut() {
            element.client_transfer_messages(&child_path, messages_vec);
        }
        if Some(self.meta_hash) != self.client_meta_hash {
            /* Transfer location metadata for this metablock to the client */
            messages_vec.push(RemoteBoxUpdate {
                parent: parent.clone(),
                new_render_blocks: smallvec![NewRenderBlock {
                    id: self.id,
                    contents: RenderBlockDescription::MetaBox(self.wire.clone())
                }],
                remove_render_blocks: Default::default(),
                move_block_locations: Default::default(),
            })
        }
    }
}

pub struct ShadowMetaContainerBlockGuard<'a> {
    container_inner: &'a mut ShadowMetaContainerBlockInner,
    idx: usize,
}
impl<'a> ShadowMetaContainerBlockGuard<'a> {
    pub fn block(&mut self) -> &mut ShadowMetaBlock {
        self.container_inner.child_blocks.get_mut(self.idx).unwrap()
    }
    pub fn location(&mut self) -> &mut RenderBlockLocation {
        self.container_inner
            .wire
            .sub_blocks
            .get_mut(self.idx)
            .unwrap()
    }
    pub fn destruct(&mut self) -> (&mut ShadowMetaBlock, &mut RenderBlockLocation) {
        (
            self.container_inner.child_blocks.get_mut(self.idx).unwrap(),
            self.container_inner
                .wire
                .sub_blocks
                .get_mut(self.idx)
                .unwrap(),
        )
    }
}

impl<'a> Drop for ShadowMetaContainerBlockGuard<'a> {
    fn drop(&mut self) {
        self.container_inner.check_changed(self.idx);
    }
}

impl ShadowMetaBlock {
    pub fn client_transfer_messages(
        &mut self,
        parent: &RenderBlockPath, // nb remember to append the id of this box for children
        messages_vec: &mut Vec<RemoteBoxUpdate>,
    ) {
        match self {
            ShadowMetaBlock::WrappedContainer(_) => todo!(),
            ShadowMetaBlock::Container(_) => todo!(),
            ShadowMetaBlock::Draw(_) => todo!(),
            ShadowMetaBlock::Text(_) => todo!(),
        }
    }
    pub fn text(&self) -> Option<&ShadowMetaTextBlock> {
        if let ShadowMetaBlock::Text(t) = self {
            Some(t)
        } else {
            None
        }
    }
    pub fn text_mut(&mut self) -> Option<&mut ShadowMetaTextBlock> {
        if let ShadowMetaBlock::Text(t) = self {
            Some(t)
        } else {
            None
        }
    }
}

impl ShadowMetaTextBlock {
    pub fn new() -> Self {
        Self {
            wire: ShapedTextBlock::default(),
            hash: None,
            client_hash: None,
        }
    }
    pub fn set_wire(&mut self, wire: ShapedTextBlock) {
        self.wire = wire;
    }
}
