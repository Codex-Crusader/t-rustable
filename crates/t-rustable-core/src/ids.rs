//! Stable page and object IDs (ADR 0005), and the map from IDs to engine indices.
//!
//! IDs of the original document come from a seed, so a replay gives the same IDs. A page
//! gets the IDs of its objects before the first change to its content.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

macro_rules! hex_id {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u128);

        impl $name {
            /// A new random ID, for a command that makes a page or an object.
            pub fn random() -> Self {
                let mut b = [0u8; 16];
                getrandom::fill(&mut b).expect("the OS random source failed");
                Self(u128::from_le_bytes(b))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:032x}", self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({self})", stringify!($name))
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("{s:?} is not a valid ID"));
                }
                u128::from_str_radix(s, 16)
                    .map(Self)
                    .map_err(|e| e.to_string())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

hex_id!(PageId);
hex_id!(ObjectId);

/// Seed for the IDs of the original document. The journal stores it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub [u8; 32]);

impl Seed {
    pub fn random() -> Self {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).expect("the OS random source failed");
        Self(b)
    }

    fn derive(&self, parts: &[&[u8]]) -> u128 {
        let mut h = Sha256::new();
        h.update(self.0);
        for p in parts {
            h.update((p.len() as u64).to_le_bytes());
            h.update(p);
        }
        let out: [u8; 32] = h.finalize().into();
        u128::from_le_bytes(out[..16].try_into().unwrap())
    }

    pub fn page(&self, original_index: usize) -> PageId {
        PageId(self.derive(&[b"page", &(original_index as u64).to_le_bytes()]))
    }

    pub fn object(&self, page: PageId, index: usize) -> ObjectId {
        ObjectId(self.derive(&[
            b"object",
            &page.0.to_le_bytes(),
            &(index as u64).to_le_bytes(),
        ]))
    }

    pub fn annotation(&self, page: PageId, index: usize) -> ObjectId {
        ObjectId(self.derive(&[
            b"annotation",
            &page.0.to_le_bytes(),
            &(index as u64).to_le_bytes(),
        ]))
    }
}

impl fmt::Debug for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seed(..)")
    }
}

impl Serialize for Seed {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&crate::assets::to_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for Seed {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        crate::assets::from_hex(&s)
            .map(Seed)
            .ok_or_else(|| serde::de::Error::custom("bad seed"))
    }
}

/// The IDs of the objects and annotations of one page, in engine index order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageContent {
    pub objects: Vec<ObjectId>,
    pub annotations: Vec<AnnotationEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnotationEntry {
    pub id: ObjectId,
    /// A form field widget. API 0.1 cannot remove these.
    pub widget: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageEntry {
    pub id: PageId,
    /// Quarter turns clockwise. None until the core reads the page.
    pub rotation: Option<u8>,
    /// None until the core reads the page.
    pub content: Option<PageContent>,
}

/// Page order and object IDs. Entry `i` of `pages` is engine page index `i`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IdMap {
    pub pages: Vec<PageEntry>,
}

/// Where an object is: page index and engine index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSlot {
    Object {
        page: usize,
        index: usize,
    },
    Annotation {
        page: usize,
        index: usize,
        widget: bool,
    },
}

impl IdMap {
    pub fn original(seed: &Seed, page_count: usize) -> Self {
        let pages = (0..page_count)
            .map(|i| PageEntry {
                id: seed.page(i),
                rotation: None,
                content: None,
            })
            .collect();
        Self { pages }
    }

    pub fn page_index(&self, id: PageId) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }

    pub fn page_ids(&self) -> Vec<PageId> {
        self.pages.iter().map(|p| p.id).collect()
    }

    /// Finds an object among the pages that have content IDs.
    pub fn find_object(&self, id: ObjectId) -> Option<(PageId, ObjectSlot)> {
        self.pages.iter().enumerate().find_map(|(page, p)| {
            let c = p.content.as_ref()?;
            if let Some(index) = c.objects.iter().position(|o| *o == id) {
                return Some((p.id, ObjectSlot::Object { page, index }));
            }
            let index = c.annotations.iter().position(|a| a.id == id)?;
            let widget = c.annotations[index].widget;
            Some((
                p.id,
                ObjectSlot::Annotation {
                    page,
                    index,
                    widget,
                },
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_as_hex() {
        let id = PageId(0x0123_4567_89ab_cdef_0000_0000_0000_002a);
        let text = id.to_string();
        assert_eq!(text, "0123456789abcdef000000000000002a");
        assert_eq!(text.parse::<PageId>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
        assert!("xyz".parse::<ObjectId>().is_err());
    }

    #[test]
    fn derived_ids_are_stable_and_distinct() {
        let seed = Seed([7; 32]);
        assert_eq!(seed.page(3), Seed([7; 32]).page(3));
        assert_ne!(seed.page(3), seed.page(4));
        assert_ne!(seed.page(3), Seed([8; 32]).page(3));
        let p = seed.page(0);
        assert_ne!(seed.object(p, 0), seed.annotation(p, 0));
        assert_ne!(seed.object(p, 0), seed.object(seed.page(1), 0));
        let back: Seed = serde_json::from_str(&serde_json::to_string(&seed).unwrap()).unwrap();
        assert_eq!(back, seed);
    }

    #[test]
    fn find_object_reports_the_slot() {
        let seed = Seed([1; 32]);
        let mut map = IdMap::original(&seed, 2);
        let p1 = map.pages[1].id;
        let (a, b) = (ObjectId(1), ObjectId(2));
        map.pages[1].content = Some(PageContent {
            objects: vec![a],
            annotations: vec![AnnotationEntry {
                id: b,
                widget: true,
            }],
        });
        assert_eq!(
            map.find_object(a),
            Some((p1, ObjectSlot::Object { page: 1, index: 0 }))
        );
        let slot = ObjectSlot::Annotation {
            page: 1,
            index: 0,
            widget: true,
        };
        assert_eq!(map.find_object(b), Some((p1, slot)));
        assert_eq!(map.find_object(ObjectId(3)), None);
    }
}
