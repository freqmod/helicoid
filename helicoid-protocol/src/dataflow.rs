use ahash::AHasher;
use hashbrown::HashSet;
use smallvec::smallvec;
use std::{
    hash::{Hash, Hasher},
    ops::Deref, any::Any,
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
pub trait AnyShadowMetaContainerBlock : Send{
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut (&mut self) -> &mut dyn Any;
    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock) -> bool;
    fn hash_value(&self) -> u64;
}

pub enum ShadowMetaBlock {
    WrappedContainer(Box<dyn AnyShadowMetaContainerBlock>),
    Container(ShadowMetaContainerBlock<()>),
    Draw(ShadowMetaDrawBlock),
    Text(ShadowMetaTextBlock),
}

impl PartialEq for ShadowMetaBlock{
    fn eq(&self, other: &Self) -> bool {
        match self{
            ShadowMetaBlock::WrappedContainer(wc) => {
                if let ShadowMetaBlock::WrappedContainer(other) = other{
                    wc.eq(other.as_ref())
                } else{
                    false
                }
            },
            ShadowMetaBlock::Container(c) => {
                if let ShadowMetaBlock::Container(other) = other{
                    PartialEq::eq(c, other)
                } else{
                    false
                }
            },
            ShadowMetaBlock::Draw(d) => {
                if let ShadowMetaBlock::Draw(other) = other{
                    d.eq(other)
                } else{
                    false
                }
                
            },
            ShadowMetaBlock::Text(t) => 
            {
                if let ShadowMetaBlock::Text(other) = other{
                    t.eq(other)
                } else{
                    false
                }
            },
        }
    }
}

impl<L> AnyShadowMetaContainerBlock for ShadowMetaContainerBlock<L> where L : Send{
    fn as_any(&self) -> &dyn Any {
        todo!()
    }

    fn as_any_mut (&mut self) -> &mut dyn Any {
        todo!()
    }

    fn eq(&self, rhs: &dyn AnyShadowMetaContainerBlock) -> bool {
        todo!()
    }

    fn hash_value(&self) -> u64 {
        todo!()
    }
}

impl Hash for ShadowMetaBlock{
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self{
            ShadowMetaBlock::WrappedContainer(wc) => {
                state.write_u64(wc.hash_value());
            },
            ShadowMetaBlock::Container(c) => {
                c.hash(state);
            },
            ShadowMetaBlock::Draw(d) => {
                d.hash(state);
            },
            ShadowMetaBlock::Text(t) => 
            {
                t.hash(state);
            },
        }
    }
}

#[derive(Hash, PartialEq)]
pub struct ShadowMetaContainerBlock<L : Send> {
    id: RenderBlockId,
    wire: MetaDrawBlock,
    child_blocks: Vec<ShadowMetaBlock>, // Corresponding index wise to the sub_blocks in wire
    hash: Option<u64>,
    client_hash: Option<u64>,
    meta_hash: u64,
    client_meta_hash: Option<u64>,
    logic: L
}

#[derive(Hash, PartialEq)]
pub struct WrappedShadowMetaContainerBlock{
    
}

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

impl<L> ShadowMetaContainerBlock<L> where L: Send{
    pub fn new(id: RenderBlockId, extent: PointF16, buffered: bool, alpha: Option<u8>, logic: L) -> Self {
        let mut s = Self {
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
            logic
        };
        s.rehash();
        s
    }
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
    pub fn child_mut(&mut self, id: RenderBlockId) -> Option<ShadowMetaContainerBlockGuard<L>> {
        if let Some(block_idx) = self
            .wire
            .sub_blocks
            .iter_mut()
            .enumerate()
            .find_map(|(idx, b)| if b.id == id { Some(idx) } else { None })
        {
            Some(ShadowMetaContainerBlockGuard {
                container: self,
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
                ShadowMetaBlock::Container(c) => c.hash,
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

pub struct ShadowMetaContainerBlockGuard<'a, L> where L: Send {
    container: &'a mut ShadowMetaContainerBlock<L>,
    idx: usize,
}
impl<'a, L> ShadowMetaContainerBlockGuard<'a, L> where L: Send {
    pub fn block(&mut self) -> &mut ShadowMetaBlock {
        self.container.child_blocks.get_mut(self.idx).unwrap()
    }
    pub fn location(&mut self) -> &mut RenderBlockLocation {
        self.container.wire.sub_blocks.get_mut(self.idx).unwrap()
    }
    pub fn destruct(&mut self) -> (&mut ShadowMetaBlock, &mut RenderBlockLocation) {
        (
            self.container.child_blocks.get_mut(self.idx).unwrap(),
            self.container.wire.sub_blocks.get_mut(self.idx).unwrap(),
        )
    }
}

impl<'a, L> Drop for ShadowMetaContainerBlockGuard<'a, L>  where L: Send {
    fn drop(&mut self) {
        self.container.check_changed(self.idx);
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
