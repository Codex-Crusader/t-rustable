//! The bundled fonts (Noto Sans, OFL). `add-text` can use only these families.

use crate::error::{CoreError, Result, code};
use skrifa::MetadataProvider;
use std::path::Path;
use std::sync::Arc;

/// Family names that select the bundled Noto Sans. The match ignores case.
pub const FAMILIES: [&str; 2] = ["Noto Sans", "sans-serif"];

pub struct Fonts {
    /// Regular, bold, italic, bold italic.
    faces: [Arc<[u8]>; 4],
}

impl Fonts {
    /// Reads the four Noto Sans files from `dir`, for example the `fonts/` folder.
    pub fn load(dir: &Path) -> Result<Self> {
        let read = |name: &str| -> Result<Arc<[u8]>> {
            let path = dir.join(name);
            let data =
                std::fs::read(&path).map_err(|e| CoreError::io(&path.display().to_string(), e))?;
            skrifa::FontRef::new(&data)
                .map_err(|e| CoreError::internal(format!("{name} is not a valid font: {e}")))?;
            Ok(data.into())
        };
        Ok(Self {
            faces: [
                read("NotoSans-Regular.ttf")?,
                read("NotoSans-Bold.ttf")?,
                read("NotoSans-Italic.ttf")?,
                read("NotoSans-BoldItalic.ttf")?,
            ],
        })
    }

    pub fn select(&self, family: &str, bold: bool, italic: bool) -> Result<Arc<[u8]>> {
        if !FAMILIES
            .iter()
            .any(|f| f.eq_ignore_ascii_case(family.trim()))
        {
            return Err(CoreError::invalid(format!(
                "The font family {family:?} is not available."
            ))
            .with_hint(format!(
                "Use one of these families: {}.",
                FAMILIES.join(", ")
            )));
        }
        Ok(self.faces[bold as usize + 2 * italic as usize].clone())
    }
}

/// Checks that the font has a glyph for each character of `text`.
pub fn check_glyphs(font: &[u8], text: &str) -> Result<()> {
    let charmap = skrifa::FontRef::new(font)
        .map_err(|e| CoreError::invalid(format!("The font is not valid: {e}")))?
        .charmap();
    let mut missing: Vec<char> = Vec::new();
    for c in text.chars() {
        if c != ' ' && charmap.map(c).is_none() && !missing.contains(&c) {
            missing.push(c);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = missing
        .iter()
        .take(10)
        .map(|c| format!("{c:?} (U+{:04X})", *c as u32))
        .collect();
    Err(CoreError::new(
        code::MISSING_GLYPH,
        "The font cannot show some characters of the text.",
    )
    .with_hint(format!(
        "Remove these characters or use another font: {}.",
        list.join(", ")
    )))
}

/// Width of `text` in points at `size`: the sum of the glyph advances, without kerning.
/// The engine lays out `AddText` in the same way.
pub fn text_width(font: &[u8], text: &str, size: f64) -> Result<f64> {
    let max = crate::command::MAX_FONT_SIZE;
    if !(size.is_finite() && size > 0.0 && size <= max) {
        return Err(CoreError::invalid(format!(
            "The font size must be more than 0 and not more than {max}. It is {size}."
        )));
    }
    check_glyphs(font, text)?;
    let font = skrifa::FontRef::new(font)
        .map_err(|e| CoreError::invalid(format!("The font is not valid: {e}")))?;
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(
        skrifa::instance::Size::new(size as f32),
        skrifa::instance::LocationRef::default(),
    );
    Ok(text
        .chars()
        .filter_map(|c| charmap.map(c))
        .filter_map(|g| metrics.advance_width(g))
        .map(f64::from)
        .sum())
}

#[cfg(test)]
pub(crate) fn test_fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")).expect("load fonts/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_width_adds_the_advances() {
        let f = test_fonts();
        let regular = f.select("Noto Sans", false, false).unwrap();
        let one = text_width(&regular, "a", 20.0).unwrap();
        assert!(one > 5.0 && one < 15.0, "{one}");
        let three = text_width(&regular, "aaa", 20.0).unwrap();
        assert!((three - 3.0 * one).abs() < 1e-9);
        assert!((text_width(&regular, "aaa", 40.0).unwrap() - 2.0 * three).abs() < 1e-3);
        assert_eq!(text_width(&regular, "", 20.0).unwrap(), 0.0);
        let e = text_width(&regular, "\u{65E5}", 20.0).unwrap_err();
        assert_eq!(e.code, code::MISSING_GLYPH);
        let e = text_width(&regular, "a", 0.0).unwrap_err();
        assert_eq!(e.code, code::INVALID_ARGUMENT);
    }

    #[test]
    fn selects_the_style() {
        let f = test_fonts();
        assert!(Arc::ptr_eq(
            &f.select("noto sans", true, false).unwrap(),
            &f.faces[1]
        ));
        assert!(Arc::ptr_eq(
            &f.select("sans-serif", true, true).unwrap(),
            &f.faces[3]
        ));
        assert_eq!(
            f.select("Comic Sans", false, false).unwrap_err().code,
            code::INVALID_ARGUMENT
        );
    }

    #[test]
    fn glyph_check_names_missing_characters() {
        let f = test_fonts();
        let regular = f.select("Noto Sans", false, false).unwrap();
        check_glyphs(&regular, "Hello Grüße ж €").unwrap();
        let e = check_glyphs(&regular, "a 日本").unwrap_err();
        assert_eq!(e.code, code::MISSING_GLYPH);
        assert!(e.hint.unwrap().contains("U+65E5"));
    }
}
