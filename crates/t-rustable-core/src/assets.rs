//! Images and fonts that commands use, named by SHA-256 (ADR 0008).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn from_hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != 2 * N || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetId(pub [u8; 32]);

impl AssetId {
    pub fn of(data: &[u8]) -> Self {
        Self(Sha256::digest(data).into())
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_hex(&self.0))
    }
}

impl fmt::Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssetId({self})")
    }
}

impl std::str::FromStr for AssetId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        from_hex(s)
            .map(Self)
            .ok_or_else(|| format!("{s:?} is not a SHA-256 name"))
    }
}

impl Serialize for AssetId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// The assets of one session. Data is shared, never copied.
#[derive(Clone, Default)]
pub struct Assets {
    map: HashMap<AssetId, Arc<[u8]>>,
}

impl Assets {
    pub fn add(&mut self, data: Arc<[u8]>) -> AssetId {
        let id = AssetId::of(&data);
        self.map.entry(id).or_insert(data);
        id
    }

    pub fn get(&self, id: AssetId) -> Option<&Arc<[u8]>> {
        self.map.get(&id)
    }

    pub fn contains(&self, id: AssetId) -> bool {
        self.map.contains_key(&id)
    }
}

/// Image formats that the engine reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Finds the format from the first bytes of the file.
pub fn sniff_image(data: &[u8]) -> Option<ImageFormat> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_ids_are_sha256() {
        let id = AssetId::of(b"abc");
        assert_eq!(
            id.to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(id.to_string().parse::<AssetId>().unwrap(), id);
        assert!("ba78".parse::<AssetId>().is_err());
    }

    #[test]
    fn store_keeps_one_copy() {
        let mut a = Assets::default();
        let x = a.add(Arc::from(&b"same"[..]));
        let y = a.add(Arc::from(&b"same"[..]));
        assert_eq!(x, y);
        assert_eq!(&**a.get(x).unwrap(), b"same");
    }

    #[test]
    fn sniffs_png_and_jpeg() {
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\nrest"),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(sniff_image(b"GIF89a"), None);
    }
}
