// Font cache for cosmic text using a gpu texture
// Should not have any other dependencies than:
// cosmic text (and transitively swash)
// optional depedency on wgpu (via traits for texture management)

/* Algorithm based on rectpack 2D https://github.com/TeamHypersomnia/rectpack2D,
code written from scratch based on that description */
/* Best packing performance is achieved by filling in sizes from largest (in y direction)
to smallest*/

/* Some more space could be claimed if, when a block can't be fitted a block is iterated trough to
find blocks that are adjacent where splitting them differentliy would lead to a 1.5 times block or
more with long edges (to avoid narrow blocks adjacent to eachother with similar length)*/
// (c) 2023 Frederik M. J. Vestre License: BSD, MIT or Apache 2

use std::{cmp::Ordering, collections::HashMap, hash::Hash};

pub type TextureCoordinateInt = u16;
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Hash)]
pub struct TextureCoordinate2D {
    pub x: TextureCoordinateInt,
    pub y: TextureCoordinateInt,
}

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Hash)]
pub struct PackedTexture {
    pub origin: TextureCoordinate2D,
    pub extent: TextureCoordinate2D,
}

pub fn insert_order(a: &TextureCoordinate2D, b: &TextureCoordinate2D) -> Ordering {
    let min_a = a.x.min(a.y);
    let min_b = b.x.min(b.y);
    let min_cmp = min_a.cmp(&min_b);
    let o = if min_cmp == Ordering::Equal {
        a.squared().cmp(&b.squared())
    } else {
        min_cmp
    };
    o.reverse()
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
    pub fn unused_space(&self) -> usize {
        self.rects.iter().map(|v| v.extent.squared()).sum()
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
                let inserted =
                    Self::raw_insert_inner(&mut self.rects, rect, Self::best_candidate_index);
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
        Self::raw_insert_inner(&mut self.rects, rect, Self::best_candidate_index)
    }

    fn _fast_candidate_index(
        rects: &mut Vec<PackedTexture>,
        rect: TextureCoordinate2D,
    ) -> Option<usize> {
        rects
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, r)| if r.fits(&rect) { Some(i) } else { None })
    }
    /* This extends the algorithm to be n2 for normal cases, but the contents of the loop is quite simple,
    so for small n's it should not be a problem */
    fn best_candidate_index(
        rects: &mut Vec<PackedTexture>,
        candidate_rect: TextureCoordinate2D,
    ) -> Option<usize> {
        let mut best_rect_size = TextureCoordinateInt::MAX;
        let mut best_rect_idx = None;
        for (idx, list_rect) in rects.iter().enumerate() {
            if list_rect.fits(&candidate_rect) {
                if list_rect.extent.x == candidate_rect.x && list_rect.extent.y == candidate_rect.y
                {
                    return Some(idx);
                }
                let diff_rect_x = list_rect.extent.x - candidate_rect.x;
                let diff_rect_y = list_rect.extent.y - candidate_rect.y;

                let diff_rect_min = diff_rect_x.min(diff_rect_y);
                if diff_rect_min < best_rect_size {
                    best_rect_idx = Some(idx);
                    best_rect_size = diff_rect_min;
                }
            }
        }
        return best_rect_idx;
    }
    /* Algorithm based on rectpack 2D https://github.com/TeamHypersomnia/rectpack2D */
    fn raw_insert_inner<F>(
        rects: &mut Vec<PackedTexture>,
        rect: TextureCoordinate2D,
        candidate_idx_func: F,
    ) -> Option<PackedTexture>
    where
        F: Fn(&mut Vec<PackedTexture>, TextureCoordinate2D) -> Option<usize>,
    {
        //        let candidate_idx = Self::fast_candidate_index(rects, rect);
        let candidate_idx = candidate_idx_func(rects, rect);
        let Some(candidate_idx) = candidate_idx  else {

                        /* No space found */
            return None;
        };
        let candidate = rects[candidate_idx];
        let res = Some(if candidate.extent.x == rect.x {
            if candidate.extent.y == rect.y {
                /*_Perfect fit, just use the whole looked up value */
                let candidate = rects.swap_remove(candidate_idx);
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
                rects[candidate_idx] = remaining;
                //                rects.push(remaining);
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
            rects[candidate_idx] = remaining;
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
            let x_min_diff = (candidate.extent.x - rect.x).abs_diff(rect.x)
                < (candidate.extent.y - rect.y).abs_diff(rect.y);

            let remaining_x = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x + rect.x,
                    y: candidate.origin.y,
                },
                extent: TextureCoordinate2D {
                    x: candidate.extent.x - rect.x,
                    y: if x_min_diff {
                        rect.y
                    } else {
                        candidate.extent.y
                    },
                },
            };
            let remaining_y = PackedTexture {
                origin: TextureCoordinate2D {
                    x: candidate.origin.x,
                    y: candidate.origin.y + rect.y,
                },
                extent: TextureCoordinate2D {
                    x: if x_min_diff {
                        candidate.extent.x
                    } else {
                        rect.x
                    },
                    y: candidate.extent.y - rect.y,
                },
            };
            if remaining_x.extent.squared() > remaining_y.extent.squared() {
                rects[candidate_idx] = remaining_x;
                rects.push(remaining_y);
            } else {
                rects[candidate_idx] = remaining_y;
                rects.push(remaining_x);
            }
            result
        });
        // Sort is more optimal, but slower
        //rects.sort_unstable_by(|a, b| b.extent.squared().cmp(&a.extent.squared()));

        res
    }

    #[cfg(test)]
    pub(crate) fn drain_remaining(&mut self) -> Vec<PackedTexture> {
        let mut new = Vec::new();
        std::mem::swap(&mut new, &mut self.rects);
        new
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
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    use svg::node::element::Rectangle;
    use svg::Document;

    fn write_colored_packed(filename: PathBuf, extent: TextureCoordinate2D, tex: &[PackedTexture]) {
        let mut document = Document::new().set("viewBox", (0, 0, extent.x, extent.y));
        for tex in tex {
            let rect = Rectangle::new()
                .set("x", tex.origin.x)
                .set("y", tex.origin.y)
                .set("width", tex.extent.x)
                .set("height", tex.extent.y)
                .set(
                    "style",
                    format!(
                        "stroke:black;stroke-width:1;fill:rgb({},{},32)",
                        tex.origin.x * 2,
                        tex.origin.y * 2
                    ),
                );
            document = document.add(rect);
        }
        svg::save(filename, &document).unwrap();
    }
    #[test]
    fn simple() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 128 });
        let mut added = Vec::new();
        added.push(
            cache
                .raw_insert(TextureCoordinate2D { x: 13, y: 8 })
                .unwrap(),
        );
        added.push(
            cache
                .raw_insert(TextureCoordinate2D { x: 14, y: 7 })
                .unwrap(),
        );
        added.push(
            cache
                .raw_insert(TextureCoordinate2D { x: 19, y: 31 })
                .unwrap(),
        );
        added.push(
            cache
                .raw_insert(TextureCoordinate2D { x: 7, y: 5 })
                .unwrap(),
        );
        for _ in 0..12 {
            match cache.raw_insert(TextureCoordinate2D { x: 15, y: 9 }) {
                Some(t) => added.push(t),
                None => {}
            }

            match cache.raw_insert(TextureCoordinate2D { x: 6, y: 15 }) {
                Some(t) => added.push(t),
                None => {}
            }
        }
        added.push(
            cache
                .raw_insert(TextureCoordinate2D { x: 37, y: 17 })
                .unwrap(),
        );

        for _ in 0..23 {
            match cache.raw_insert(TextureCoordinate2D { x: 16, y: 10 }) {
                Some(t) => added.push(t),
                None => {
                    assert!(false)
                }
            }
            match cache.raw_insert(TextureCoordinate2D { x: 17, y: 13 }) {
                Some(t) => added.push(t),
                None => {
                    assert!(false)
                }
            }

            match cache.raw_insert(TextureCoordinate2D { x: 4, y: 7 }) {
                Some(t) => added.push(t),
                None => {
                    assert!(false)
                }
            }
        }

        /*        println!(
            "Inserted into cache: {:?} free space: {:?}",
            added,
            cache.unused_space()
        );*/
        let remaining = cache.drain_remaining();
        println!("Cache info: {:?}", remaining);
        write_colored_packed(PathBuf::from("simpleused.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("simpletest.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn uniform() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 128 });
        let mut added = Vec::new();
        for _ in 0..50 {
            added.push(
                cache
                    .raw_insert(TextureCoordinate2D { x: 12, y: 20 })
                    .unwrap(),
            );
        }
        let remaining = cache.drain_remaining();
        write_colored_packed(PathBuf::from("uniformused.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("uniformtest.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn uniform_height() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 128 });
        let mut added = Vec::new();
        for i in 0..54 {
            added.push(
                cache
                    .raw_insert(TextureCoordinate2D {
                        x: 11 + (i % 7),
                        y: 20,
                    })
                    .unwrap(),
            );
        }
        let remaining = cache.drain_remaining();
        write_colored_packed(PathBuf::from("unihused.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("unihtest.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn uniform_height2() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 128 });
        let mut added = Vec::new();
        for i in 0..37 {
            added.push(
                cache
                    .raw_insert(TextureCoordinate2D {
                        x: 5 + (i % 20),
                        y: 20,
                    })
                    .unwrap(),
            );
        }
        let remaining = cache.drain_remaining();
        write_colored_packed(PathBuf::from("unihused2.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("unihtest2.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn uniform_height_sorted2() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 128 });
        let mut added = Vec::new();
        let mut src = Vec::new();
        for i in 0..54 {
            src.push(TextureCoordinate2D {
                x: 5 + (i % 20),
                y: 20,
            });
        }
        src.sort_by(insert_order);
        for s in src.iter() {
            added.push(cache.raw_insert(s.clone()).unwrap());
        }
        let remaining = cache.drain_remaining();
        write_colored_packed(PathBuf::from("unihuseds2.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("unihtests2.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn similar() {
        let mut cache = PackedTextureCache::<()>::new(TextureCoordinate2D { x: 128, y: 92 });
        let mut added = Vec::new();
        for i in 0..33 {
            added.push(
                cache
                    .raw_insert(TextureCoordinate2D {
                        x: 11 + (i % 7),
                        y: 22 - (i % 7),
                    })
                    .unwrap(),
            );
        }
        let remaining = cache.drain_remaining();
        write_colored_packed(PathBuf::from("similarused.svg"), cache.extent(), &added);
        write_colored_packed(PathBuf::from("similartest.svg"), cache.extent(), &remaining);
    }
    #[test]
    fn keyed() {
        let mut cache = PackedTextureCache::<u64>::new(TextureCoordinate2D { x: 128, y: 127 });

        let mut added = Vec::new();
        for i in 0..25 {
            added.push(
                cache
                    .insert(i, TextureCoordinate2D { x: 22, y: 20 })
                    .unwrap()
                    .clone(),
            );
        }
        for i in 0..25 {
            let inserted = cache
                .insert(i, TextureCoordinate2D { x: 22, y: 20 })
                .unwrap();
            assert_eq!(inserted.origin, added[i as usize].origin);
        }
    }
}
