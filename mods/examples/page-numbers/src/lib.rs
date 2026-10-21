//! Example mod: a batch job that adds page numbers.
//!
//! Each page gets a number at the bottom, as the reader sees the page. On a rotated page
//! the bottom is another edge of the crop box, and the text turns with the page. All
//! pages change in one transaction, so one undo removes all numbers.

use serde::{Deserialize, Serialize};
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::types::{ApiError, Color, Point, TextStyle};
use t_rustable_sdk::{
    BatchJob, Command, api_error, document, export_batch, parse_args, run_commands,
};

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Position {
    Left,
    Center,
    Right,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Args {
    format: String,
    position: Position,
    size: f64,
    margin: f64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            format: "Page {n} of {total}".into(),
            position: Position::Center,
            size: 10.0,
            margin: 24.0,
        }
    }
}

#[derive(Serialize)]
struct Reply {
    pages: usize,
}

fn style(size: f64) -> TextStyle {
    TextStyle {
        font_family: "Noto Sans".into(),
        size,
        color: Color::rgb(0, 0, 0),
        bold: false,
        italic: false,
    }
}

/// The baseline direction and the "down" direction of the page as the reader sees it,
/// in page coordinates, for a page rotation (clockwise).
fn axes(rotation: u32) -> ((f64, f64), (f64, f64)) {
    match rotation % 360 {
        90 => ((0.0, 1.0), (1.0, 0.0)),
        180 => ((-1.0, 0.0), (0.0, 1.0)),
        270 => ((0.0, -1.0), (-1.0, 0.0)),
        _ => ((1.0, 0.0), (0.0, -1.0)),
    }
}

/// Where the text starts on one page.
fn place(
    doc: &Doc,
    page: t_rustable_sdk::types::PageId,
    width: f64,
    args: &Args,
) -> Result<Point, ApiError> {
    let info = doc.page_info(page)?;
    let b = info.crop_box;
    let (x0, x1) = (b.left.min(b.right), b.left.max(b.right));
    let (y0, y1) = (b.bottom.min(b.top), b.bottom.max(b.top));
    let center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let ((dx, dy), (downx, downy)) = axes(info.rotation);
    // Half the page size along each direction.
    let half_along = (dx.abs() * (x1 - x0) + dy.abs() * (y1 - y0)) / 2.0;
    let half_down = (downx.abs() * (x1 - x0) + downy.abs() * (y1 - y0)) / 2.0;
    let down = half_down - args.margin;
    // Distance along the baseline from the center to the start of the text.
    let along = match args.position {
        Position::Center => -width / 2.0,
        Position::Left => -half_along + args.margin,
        Position::Right => half_along - args.margin - width,
    };
    Ok(Point::new(
        center.0 + dx * along + downx * down,
        center.1 + dy * along + downy * down,
    ))
}

struct PageNumbers;

impl BatchJob for PageNumbers {
    fn run(doc: &Doc, args: String) -> Result<String, ApiError> {
        let args: Args = parse_args::<Option<Args>>(&args)?.unwrap_or_default();
        if !(4.0..=72.0).contains(&args.size) {
            return Err(api_error(
                "invalid-argument",
                "size must be 4 to 72 points.",
            ));
        }
        if !(0.0..=200.0).contains(&args.margin) {
            return Err(api_error(
                "invalid-argument",
                "margin must be 0 to 200 points.",
            ));
        }
        let pages = doc.page_ids();
        let total = pages.len();
        let mut commands = Vec::with_capacity(total);
        for (i, &page) in pages.iter().enumerate() {
            let text = args
                .format
                .replace("{n}", &(i + 1).to_string())
                .replace("{total}", &total.to_string());
            let width = document::measure_text(&text, &style(args.size))?;
            commands.push(Command::AddText {
                page,
                at: place(doc, page, width, &args)?,
                text,
                style: style(args.size),
            });
        }
        run_commands(doc, "Add page numbers", commands)?;
        Ok(serde_json::to_string(&Reply { pages: total }).unwrap_or_default())
    }
}

export_batch!(PageNumbers);
