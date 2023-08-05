use std::{
    fmt::{self, Debug},
    path::PathBuf,
};

use swash::{CacheKey, FontRef};

use crate::font::fontcache::FontOwner;

pub struct SwashFont {
    data: Vec<u8>,
    offset: u32,
    pub key: CacheKey,
}

impl Debug for SwashFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SwashFont")
            .field("offset", &self.offset)
            .field("key", &self.key)
            .finish()
    }
}

impl SwashFont {
    pub fn from_path(path: &PathBuf, index: usize) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::from_data(data, index)
    }
    pub fn from_data(data: Vec<u8>, index: usize) -> Option<Self> {
        let font = FontRef::from_index(&data, index)?;
        let (offset, key) = (font.offset, font.key);
        Some(Self { data, offset, key })
    }

    pub fn as_ref(&self) -> FontRef {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }
}
impl FontOwner for SwashFont {
    fn swash_font(&self) -> FontRef<'_> {
        self.as_ref()
    }
}
