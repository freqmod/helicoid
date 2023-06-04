// Font cache for cosmic text using a gpu texture
// Should not have any other dependencies than:
// cosmic text (and transitively swash)
// optional depedency on wgpu (via traits for texture management)

/* Algorithm based on rectpack 2D https://github.com/TeamHypersomnia/rectpack2D,
code written from scratch based on that description */
//
// (c) 2023 Frederik M. J. Vestre License: BSD, MIT or Apache 2

use std::{collections::HashMap, hash::Hash};

pub type TextureCoordinateInt = u16;
#[derive(Copy, Clone, Default, Debug)]
pub struct TextureCoordinate2D {
    pub x: TextureCoordinateInt,
    pub y: TextureCoordinateInt,
}

#[derive(Copy, Clone, Default, Debug)]
pub struct PackedTexture {
    pub origin: TextureCoordinate2D,
    pub extent: TextureCoordinate2D,
}

#[derive(Clone, Default, Debug)]
pub struct PackedTextureCache<K>
where
    K: PartialEq + Eq + Hash,
{
    extent: TextureCoordinate2D,
    rects: Vec<PackedTexture>,
    cached: HashMap<K, PackedTexture>,
}
impl<K> PackedTextureCache<K>
where
    K: PartialEq + Eq + Hash,
{
    pub fn new(extent: TextureCoordinate2D) -> Self {
        Self {
            extent,
            rects: vec![PackedTexture {
                origin: TextureCoordinate2D::zero(),
                extent,
            }],
            cached: HashMap::new(),
        }
    }
    pub fn extent(&self) -> TextureCoordinate2D {
        self.extent
    }
    /* Extend the cache to new size (which must be bigger than currrent size), leaving the current content as is */
    pub fn extend(&mut self, rect: TextureCoordinate2D) -> Result<(), ()> {
        if rect.x < self.extent.x || rect.y < self.extent.y {
            return Err(());
        }
        /* Assume the new element are largest, so add them at the start */
        let old_extent = self.extent;
        self.extent = rect;

        let remaining_x = PackedTexture {
            origin: TextureCoordinate2D {
                x: 0,
                y: old_extent.y,
            },
            extent: TextureCoordinate2D {
                x: self.extent.x,
                y: self.extent.y - old_extent.y,
            },
        };
        let remaining_y = PackedTexture {
            origin: TextureCoordinate2D {
                x: old_extent.x,
                y: 0,
            },
            extent: TextureCoordinate2D {
                x: self.extent.x - old_extent.x,
                y: self.extent.y,
            },
        };
        /* This is not very efficent, but resize should happen seldom anyhow */
        if remaining_x.extent.squared() > remaining_y.extent.squared() {
            self.rects.insert(0, remaining_y);
            self.rects.insert(0, remaining_x);
        } else {
            self.rects.insert(0, remaining_x);
            self.rects.insert(0, remaining_y);
        }
        Ok(())
    }
    pub fn insert(&mut self, key: K, rect: TextureCoordinate2D) -> Option<&PackedTexture> {
        let entry = self.cached.entry(key);
        match entry {
            std::collections::hash_map::Entry::Occupied(val) => Some(val.into_mut()),
            std::collections::hash_map::Entry::Vacant(val) => {
                /* Calling raw_insert_inner as a class function to avoid borrowing issues with the cache hashmap */
                let inserted = Self::raw_insert_inner(&mut self.rects, rect);
                if let Some(inserted) = inserted {
                    let valref = val.insert(inserted);
                    Some(valref)
                } else {
                    None
                }
            }
        }
    }

    pub fn raw_insert(&mut self, rect: TextureCoordinate2D) -> Option<PackedTexture> {
        Self::raw_insert_inner(&mut self.rects, rect)
    }

    /* Algorithm based on rectpack 2D https://github.com/TeamHypersomnia/rectpack2D */
    fn raw_insert_inner(
        rects: &mut Vec<PackedTexture>,
        rect: TextureCoordinate2D,
    ) -> Option<PackedTexture> {
        let candidate_idx =
            rects
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, r)| if r.fits(&rect) { Some(i) } else { None });
        let Some(candidate_idx) = candidate_idx  else {
            /* No space found */
            return None;
        };
        let candidate = rects.swap_remove(candidate_idx);
        Some(if candidate.extent.x == rect.x {
            if candidate.extent.y == rect.y {
                /*_Perfect fit, just use the whole looked up value */
                candidate
            } else {
                /* Perfect fit in x-direction, add one square only */
                let result = PackedTexture {
                    origin: TextureCoordinate2D {
                        x: candidate.origin.x,
                        y: candidate.origin.y,
                    },
                    extent: rect,
                };
                let remaining = PackedTexture {
                    origin: TextureCoordinate2D {
                        x: candidate.origin.x,
                        y: candidate.origin.y + rect.y,
                    },
                    extent: TextureCoordinate2D {
                        x: candidate.extent.x,
                        y: candidate.extent.y - rect.y,
                    },
                };
                rects.push(remaining);
                result
            }
        } else if candidate.extent.y == rect.y {
            /* Perfect fit in y-direction, add one square only */
            let result = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x,
                    y: candidate.origin.y,
                },
                extent: rect,
            };
            let remaining = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x + rect.x,
                    y: candidate.origin.y,
                },
                extent: TextureCoordinate2D {
                    x: candidate.extent.x - rect.x,
                    y: candidate.extent.y,
                },
            };
            rects.push(remaining);
            result
        } else {
            /* The most likely, non-perfect fit in either direction, add two smaller blocks */
            let result = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x,
                    y: candidate.origin.y,
                },
                extent: rect,
            };
            let remaining_x = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x + rect.x,
                    y: candidate.origin.y,
                },
                extent: TextureCoordinate2D {
                    x: candidate.extent.x - rect.x,
                    y: candidate.extent.y,
                },
            };
            let remaining_y = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x,
                    y: candidate.origin.y + rect.y,
                },
                extent: TextureCoordinate2D {
                    x: candidate.extent.x,
                    y: candidate.extent.y - rect.y,
                },
            };
            if remaining_x.extent.squared() > remaining_y.extent.squared() {
                rects.push(remaining_x);
                rects.push(remaining_y);
            } else {
                rects.push(remaining_y);
                rects.push(remaining_x);
            }
            result
        })
    }
}
impl TextureCoordinate2D {
    pub fn zero() -> Self {
        Self { x: 0, y: 0 }
    }
    fn bigger(&self, other: &Self) -> bool {
        self.x >= other.x && self.y >= other.y
    }
    fn squared(&self) -> usize {
        self.x as usize * self.y as usize
    }
}
impl PackedTexture {
    fn fits(&self, extent: &TextureCoordinate2D) -> bool {
        self.extent.bigger(extent)
    }
}
