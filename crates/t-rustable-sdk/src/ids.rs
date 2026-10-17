//! Text form, equality, and hashing for page and object IDs: 32 lowercase hex digits.

use crate::types::{ObjectId, PageId};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

macro_rules! id_impls {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:016x}{:016x}", self.hi, self.lo)
            }
        }

        impl FromStr for $ty {
            type Err = crate::types::ApiError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let bad =
                    || crate::api_error("invalid-argument", format!("{s:?} is not a valid ID."));
                if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(bad());
                }
                let hi = u64::from_str_radix(&s[..16], 16).map_err(|_| bad())?;
                let lo = u64::from_str_radix(&s[16..], 16).map_err(|_| bad())?;
                Ok(Self { hi, lo })
            }
        }

        impl PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                (self.hi, self.lo) == (other.hi, other.lo)
            }
        }

        impl Eq for $ty {}

        impl Hash for $ty {
            fn hash<H: Hasher>(&self, state: &mut H) {
                (self.hi, self.lo).hash(state);
            }
        }
    };
}

id_impls!(PageId);
id_impls!(ObjectId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let id = PageId {
            hi: 0x0123_4567_89ab_cdef,
            lo: 42,
        };
        let text = id.to_string();
        assert_eq!(text, "0123456789abcdef000000000000002a");
        assert_eq!(text.parse::<PageId>().unwrap(), id);
    }

    #[test]
    fn rejects_bad_text() {
        assert!("xyz".parse::<ObjectId>().is_err());
        assert!(
            "g123456789abcdef000000000000002a"
                .parse::<ObjectId>()
                .is_err()
        );
    }
}
