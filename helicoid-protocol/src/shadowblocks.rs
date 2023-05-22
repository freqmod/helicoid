use ahash::AHasher;
use smallvec::SmallVec;
use std::{
    any::Any,
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use crate::{
    gfx::{
        MetaDrawBlock, NewRenderBlock, PointF32, RenderBlockDescription, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, SimpleDrawBlock,
    },
    text::ShapedTextBlock,
    transferbuffer::TransferBuffer,
};
/* Type erased Container (inspired by Xilem) */
pub trait AnyShadowMetaContainerBlock<C>: Send
where
    C: VisitingContext,
{
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock<C>) -> bool;
    fn hash_value(&self) -> u64;
    fn inner(&self) -> &ShadowMetaContainerBlockInner<C>;
    fn inner_mut(&mut self) -> &mut ShadowMetaContainerBlockInner<C>;
    fn initialize(&mut self, context: &mut C);
    fn update(&mut self, context: &mut C);
}

pub trait VisitingContext: Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
pub trait ContainerBlockLogic: Send + Hash + PartialEq {
    type UpdateContext: VisitingContext;
    /* TODO: Should we have an init function, and possible a finalize funtion too? */
    fn initialize(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized;
    fn pre_update(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized;
    fn post_update(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized;
}

/* Container block logic, without any logic, used as a filler when setting up a
container that has no real logic associated */
pub struct NoContainerBlockLogic<C> {
    context_type: PhantomData<C>,
}

impl<C> Hash for NoContainerBlockLogic<C> {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}
impl<C> PartialEq for NoContainerBlockLogic<C> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl<C> Default for NoContainerBlockLogic<C> {
    fn default() -> Self {
        Self {
            context_type: PhantomData,
        }
    }
}
impl<C> ContainerBlockLogic for NoContainerBlockLogic<C>
where
    C: VisitingContext,
{
    type UpdateContext = C;
    fn pre_update(
        _block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        _context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }
    fn post_update(
        _block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        _context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }

    fn initialize(
        _block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        _context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }
}

pub enum ShadowMetaBlock<C>
where
    C: VisitingContext,
{
    WrappedContainer(Box<dyn AnyShadowMetaContainerBlock<C>>),
    Container(ShadowMetaContainerBlock<NoContainerBlockLogic<C>, C>),
    Draw(ShadowMetaDrawBlock),
    Text(ShadowMetaTextBlock),
}

impl<C> PartialEq for ShadowMetaBlock<C>
where
    C: VisitingContext,
{
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

impl<L> AnyShadowMetaContainerBlock<L::UpdateContext>
    for ShadowMetaContainerBlock<L, L::UpdateContext>
where
    L: ContainerBlockLogic + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }

    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock<L::UpdateContext>) -> bool {
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

    fn inner(&self) -> &ShadowMetaContainerBlockInner<L::UpdateContext> {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut ShadowMetaContainerBlockInner<L::UpdateContext> {
        &mut self.inner
    }

    fn initialize(&mut self, context: &mut L::UpdateContext) {
        <ShadowMetaContainerBlock<L, L::UpdateContext>>::initialize(
            self,
            context as &mut L::UpdateContext,
        )
    }

    fn update(&mut self, context: &mut L::UpdateContext) {
        <ShadowMetaContainerBlock<L, L::UpdateContext>>::update(self, context)
    }
}

impl<C> Hash for ShadowMetaBlock<C>
where
    C: VisitingContext,
{
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

pub struct ShadowMetaContainerBlockInner<C>
where
    C: VisitingContext,
{
    id: RenderBlockId,
    wire: MetaDrawBlock,
    child_blocks: Vec<ShadowMetaBlock<C>>, // Corresponding index wise to the sub_blocks in wire
    pending_removal: SmallVec<[RenderBlockId; 8]>,
    hash: Option<u64>,
    client_hash: Option<u64>,
    meta_hash: u64,
    client_meta_hash: Option<u64>,
    location: Option<RenderBlockLocation>,
}

pub struct ShadowMetaContainerBlock<L, C>
where
    L: ContainerBlockLogic,
    C: VisitingContext,
{
    inner: ShadowMetaContainerBlockInner<C>,
    logic: L,
}

impl<L, C> Hash for ShadowMetaContainerBlock<L, C>
where
    L: ContainerBlockLogic,
    C: VisitingContext,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
        self.logic.hash(state);
    }
}

impl<L, C> PartialEq for ShadowMetaContainerBlock<L, C>
where
    L: ContainerBlockLogic,
    C: VisitingContext,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner) && self.logic.eq(&other.logic)
    }
}
impl<C> Hash for ShadowMetaContainerBlockInner<C>
where
    C: VisitingContext,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.wire.hash(state);
        self.child_blocks.hash(state);
        self.hash.hash(state);
        self.client_hash.hash(state);
        self.meta_hash.hash(state);
        self.client_meta_hash.hash(state);
    }
}
impl<C> PartialEq for ShadowMetaContainerBlockInner<C>
where
    C: VisitingContext,
{
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
            && self.wire.eq(&other.wire)
            && self.child_blocks.eq(&other.child_blocks)
            && self.hash.eq(&other.hash)
            && self.client_hash.eq(&other.client_hash)
            && self.meta_hash.eq(&other.meta_hash)
            && self.client_meta_hash.eq(&other.client_meta_hash)
    }
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
    id: RenderBlockId,
    hash: Option<u64>,
    client_hash: Option<u64>,
    location: Option<RenderBlockLocation>,
}

impl<L> ShadowMetaContainerBlock<L, L::UpdateContext>
where
    L: ContainerBlockLogic + 'static,
{
    pub fn new(
        id: RenderBlockId,
        extent: PointF32,
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
                pending_removal: Default::default(),
                hash: None,
                client_hash: None,
                meta_hash: 0,
                client_meta_hash: None,
                location: None,
            },
            logic,
        };
        s.inner.rehash();
        s
    }
    pub fn inner_ref(&self) -> &ShadowMetaContainerBlockInner<L::UpdateContext> {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut ShadowMetaContainerBlockInner<L::UpdateContext> {
        &mut self.inner
    }
    pub fn logic_ref(&self) -> &L {
        &self.logic
    }
    pub fn logic_mut(&mut self) -> &mut L {
        &mut self.logic
    }
    pub fn destruct_mut(
        &mut self,
    ) -> (&mut ShadowMetaContainerBlockInner<L::UpdateContext>, &mut L) {
        let Self { inner, logic } = self;
        (inner, logic)
    }
    pub fn extent(&self) -> PointF32 {
        self.inner.wire.extent
    }
    pub fn set_extent(&mut self, extent: PointF32) {
        self.inner.wire.extent = extent;
    }
    pub fn alpha(&mut self) -> Option<u8> {
        self.inner.wire.alpha
    }
    pub fn set_alpha(&mut self, alpha: Option<u8>) {
        self.inner.wire.alpha = alpha;
    }
    pub fn set_child(
        &mut self,
        location: RenderBlockLocation,
        block: ShadowMetaBlock<L::UpdateContext>,
    ) {
        self.inner.set_child(location, block)
    }
    pub fn remove_child(
        &mut self,
        id: RenderBlockId,
    ) -> Option<(ShadowMetaBlock<L::UpdateContext>, RenderBlockLocation)> {
        self.inner.remove_child(id)
    }
    pub fn child(
        &self,
        id: RenderBlockId,
    ) -> Option<(&ShadowMetaBlock<L::UpdateContext>, &RenderBlockLocation)> {
        self.inner.child(id)
    }
    pub fn child_mut(
        &mut self,
        id: RenderBlockId,
    ) -> Option<ShadowMetaContainerBlockGuard<L::UpdateContext>> {
        self.inner.child_mut(id)
    }
    pub fn initialize(&mut self, context: &mut L::UpdateContext) {
        <L as ContainerBlockLogic>::initialize(self, context);
        self.inner.initialize_children(context);
    }
    pub fn update(&mut self, context: &mut L::UpdateContext) {
        <L as ContainerBlockLogic>::pre_update(self, context);
        self.inner.update_children(context);
        <L as ContainerBlockLogic>::post_update(self, context);
    }
}
impl<C> ShadowMetaContainerBlockInner<C>
where
    C: VisitingContext + 'static,
{
    pub fn update_children(&mut self, context: &mut C) {
        for element in self.child_blocks.iter_mut() {
            match element {
                ShadowMetaBlock::WrappedContainer(wc) => {
                    wc.update(context);
                }
                ShadowMetaBlock::Container(c) => {
                    c.update(context);
                }
                ShadowMetaBlock::Draw(_) | ShadowMetaBlock::Text(_) => {}
            }
        }
    }
    pub fn initialize_children(&mut self, context: &mut C) {
        for element in self.child_blocks.iter_mut() {
            match element {
                ShadowMetaBlock::WrappedContainer(wc) => {
                    wc.initialize(context);
                }
                ShadowMetaBlock::Container(c) => {
                    c.initialize(context);
                }
                ShadowMetaBlock::Draw(_) | ShadowMetaBlock::Text(_) => {}
            }
        }
    }
    pub fn extent(&self) -> PointF32 {
        self.wire.extent
    }
    pub fn set_extent(&mut self, extent: PointF32) {
        self.wire.extent = extent;
    }
    pub fn alpha(&mut self) -> Option<u8> {
        self.wire.alpha
    }
    pub fn set_alpha(&mut self, alpha: Option<u8>) {
        self.wire.alpha = alpha;
    }
    pub fn set_child(&mut self, location: RenderBlockLocation, block: ShadowMetaBlock<C>) {
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
    ) -> Option<(ShadowMetaBlock<C>, RenderBlockLocation)> {
        let res = if let Some(block_idx) = self
            .wire
            .sub_blocks
            .iter()
            .enumerate()
            .find_map(|(idx, b)| if b.id == id { Some(idx) } else { None })
        {
            let removed = Some((
                self.child_blocks.remove(block_idx),
                self.wire.sub_blocks.remove(block_idx),
            ));
            /* Notify client about the removal next time it is synced */
            //log::trace!("Add pending removal: {}", id.0);
            self.pending_removal.push(id);
            removed
        } else {
            None
        };
        self.rehash();
        res
    }
    pub fn child(&self, id: RenderBlockId) -> Option<(&ShadowMetaBlock<C>, &RenderBlockLocation)> {
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
    pub fn child_mut(&mut self, id: RenderBlockId) -> Option<ShadowMetaContainerBlockGuard<C>> {
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

        self.pending_removal.hash(&mut hasher);

        let new_hash = hasher.finish();
        self.hash = Some(new_hash);
    }
    pub fn client_transfer_messages(
        &mut self,
        parent: &RenderBlockPath, // nb remember to append the id of this box for children
        location: &mut RenderBlockLocation,
        transfer_buffer: &mut TransferBuffer,
    ) {
        log::trace!(
            "CTM: P:{:?} I: {:?} #CL: {:?} #WL:{:?} PR: {:?}",
            parent,
            self.id,
            self.child_blocks.len(),
            self.wire.sub_blocks.len(),
            self.pending_removal,
        );
        /* Make messages that transfers all outstanding state to client */
        if self.hash.is_none() {
            self.rehash();
        }
        if self.client_hash.is_some()
            && self.hash == self.client_hash
            && self
                .location
                .as_ref()
                .map(|l| l == location)
                .unwrap_or(false)
        {
            return;
        }
        self.location = Some(location.clone());
        let child_path = RenderBlockPath::child(parent, self.id);
        if Some(self.meta_hash) != self.client_meta_hash {
            if !self.pending_removal.is_empty() {
                transfer_buffer.add_removes(&child_path, &self.pending_removal);
            }
            /* Transfer location metadata for this metablock to the client */
            transfer_buffer.add_news(
                &parent,
                &[NewRenderBlock {
                    id: self.id,
                    contents: RenderBlockDescription::MetaBox(self.wire.clone()),
                    update: true,
                }],
            );
            transfer_buffer.add_moves(&parent, &[self.location.clone().unwrap()]);
        }
        /* Push contents after the outside block, to ensure that the client knows about them */
        for (idx, element) in self.child_blocks.iter_mut().enumerate() {
            element.client_transfer_messages(
                &child_path,
                self.wire.sub_blocks.get_mut(idx).unwrap(),
                transfer_buffer,
            );
        }
        /* Make sure to update hash to cover any changes that have been pushed to the client */
        self.rehash();
    }
    pub fn id(&self) -> RenderBlockId {
        self.id
    }
}

pub struct ShadowMetaContainerBlockGuard<'a, C>
where
    C: VisitingContext + 'static,
{
    container_inner: &'a mut ShadowMetaContainerBlockInner<C>,
    idx: usize,
}
impl<'a, C> ShadowMetaContainerBlockGuard<'a, C>
where
    C: VisitingContext,
{
    pub fn block(&mut self) -> &mut ShadowMetaBlock<C> {
        self.container_inner.child_blocks.get_mut(self.idx).unwrap()
    }
    pub fn location(&mut self) -> &mut RenderBlockLocation {
        self.container_inner
            .wire
            .sub_blocks
            .get_mut(self.idx)
            .unwrap()
    }
    pub fn destruct(&mut self) -> (&mut ShadowMetaBlock<C>, &mut RenderBlockLocation) {
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

impl<'a, C> Drop for ShadowMetaContainerBlockGuard<'a, C>
where
    C: VisitingContext + 'static,
{
    fn drop(&mut self) {
        self.container_inner.check_changed(self.idx);
    }
}

impl<C> ShadowMetaBlock<C>
where
    C: VisitingContext + 'static,
{
    /*fn client_transfer_container<'a>(
        container: &'a mut dyn AnyShadowMetaContainerBlock<C>,
        parent: &RenderBlockPath,
        messages_vec: &mut Vec<RemoteBoxUpdate>,
    ) {
    }*/
    pub fn client_transfer_messages(
        &mut self,
        parent: &RenderBlockPath, // nb remember to append the id of this box for children
        location: &mut RenderBlockLocation,
        transfer_buffer: &mut TransferBuffer,
    ) {
        match self {
            ShadowMetaBlock::WrappedContainer(wc) => {
                wc.inner_mut()
                    .client_transfer_messages(parent, location, transfer_buffer)
            }
            ShadowMetaBlock::Container(c) => {
                c.inner_mut()
                    .client_transfer_messages(parent, location, transfer_buffer)
            }
            ShadowMetaBlock::Draw(_) => todo!(),
            ShadowMetaBlock::Text(t) => {
                t.client_transfer_messages(parent, location, transfer_buffer)
            }
        }
    }
    pub fn extent_mut(&mut self) -> &mut PointF32 {
        match self {
            ShadowMetaBlock::WrappedContainer(wc) => &mut wc.inner_mut().wire.extent,
            ShadowMetaBlock::Container(c) => &mut c.inner.wire.extent,
            ShadowMetaBlock::Draw(d) => &mut d.wire.extent,
            ShadowMetaBlock::Text(t) => &mut t.wire.extent,
        }
    }
    pub fn extent(&self) -> &PointF32 {
        match self {
            ShadowMetaBlock::WrappedContainer(wc) => &wc.inner().wire.extent,
            ShadowMetaBlock::Container(c) => &c.inner.wire.extent,
            ShadowMetaBlock::Draw(d) => &d.wire.extent,
            ShadowMetaBlock::Text(t) => &t.wire.extent,
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
    pub fn container(&self) -> Option<&(dyn AnyShadowMetaContainerBlock<C> + 'static)> {
        if let ShadowMetaBlock::Container(c) = self {
            Some(c)
        } else if let ShadowMetaBlock::WrappedContainer(ref wc) = self {
            Some(wc.as_ref())
        } else {
            None
        }
    }
    pub fn container_mut(&mut self) -> Option<&mut (dyn AnyShadowMetaContainerBlock<C> + 'static)> {
        if let ShadowMetaBlock::Container(c) = self {
            Some(c)
        } else if let ShadowMetaBlock::WrappedContainer(ref mut wc) = self {
            Some(wc.as_mut())
        } else {
            None
        }
    }
}

impl ShadowMetaTextBlock {
    pub fn new(id: RenderBlockId) -> Self {
        Self {
            wire: ShapedTextBlock::default(),
            id,
            hash: None,
            client_hash: None,
            location: None,
        }
    }
    pub fn set_wire(&mut self, wire: ShapedTextBlock) {
        self.wire = wire;
    }
    fn rehash(&mut self) {
        let mut hasher = AHasher::default();
        //self.hash(&mut hasher);
        self.wire.hash(&mut hasher);
        self.id.hash(&mut hasher);
        //        self.location.hash(&mut hasher);
        self.hash = Some(hasher.finish());
    }
    pub fn id(&self) -> RenderBlockId {
        self.id
    }
    pub fn client_transfer_messages(
        &mut self,
        parent: &RenderBlockPath, // nb remember to append the id of this box for children
        location: &mut RenderBlockLocation,
        transfer_buffer: &mut TransferBuffer,
    ) {
        /* Make messages that transfers all outstanding state to client */
        if self.hash.is_none() {
            self.rehash();
        }
        if self.client_hash.is_some()
            && self.hash == self.client_hash
            && self
                .location
                .as_ref()
                .map(|l| l == location)
                .unwrap_or(false)
        {
            log::trace!("Skip transferring text block due to hash reuse");
            return;
        }
        self.location = Some(location.clone());
        self.client_hash = self.hash; // This doesn't work, probably a parent is removed
                                      /* Transfer location metadata for this metablock to the client */
        transfer_buffer.add_news(
            &parent,
            &[NewRenderBlock {
                id: self.id,
                contents: RenderBlockDescription::ShapedTextBlock(self.wire.clone()),
                update: true,
            }],
        );
        transfer_buffer.add_moves(&parent, &[self.location.clone().unwrap()]);
    }
}
