//! Commands as Rust data. The API itself has one `transaction` method per command,
//! so new commands can come in later API versions without breaking old mods.

use crate::document::Doc;
use crate::edit::{self, Shape, Transaction};
use crate::types::{
    ApiError, Color, ObjectId, PageId, Point, Rect, Stroke, TextStyle, TransactionId,
};

/// One core command. New variants come in later SDK versions.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Command {
    DeletePage(PageId),
    MovePage {
        page: PageId,
        before: Option<PageId>,
    },
    RotatePage {
        page: PageId,
        degrees: i32,
    },
    InsertBlankPage {
        before: Option<PageId>,
        width: f64,
        height: f64,
    },
    AddText {
        page: PageId,
        at: Point,
        text: String,
        style: TextStyle,
    },
    AddImage {
        page: PageId,
        bounds: Rect,
        data: Vec<u8>,
    },
    AddShape {
        page: PageId,
        shape: Shape,
        stroke: Option<Stroke>,
        fill: Option<Color>,
    },
    RemoveObject(ObjectId),
}

/// What a command made, if anything.
#[derive(Debug, Clone, PartialEq)]
pub enum Created {
    Nothing,
    Page(PageId),
    Object(ObjectId),
}

impl Command {
    /// Adds this command to an open transaction.
    pub fn apply(self, tx: &Transaction) -> Result<Created, ApiError> {
        Ok(match self {
            Command::DeletePage(page) => tx.delete_page(page).map(|_| Created::Nothing)?,
            Command::MovePage { page, before } => {
                tx.move_page(page, before).map(|_| Created::Nothing)?
            }
            Command::RotatePage { page, degrees } => {
                tx.rotate_page(page, degrees).map(|_| Created::Nothing)?
            }
            Command::InsertBlankPage {
                before,
                width,
                height,
            } => Created::Page(tx.insert_blank_page(before, width, height)?),
            Command::AddText {
                page,
                at,
                text,
                style,
            } => Created::Object(tx.add_text(page, at, &text, &style)?),
            Command::AddImage { page, bounds, data } => {
                Created::Object(tx.add_image(page, bounds, &data)?)
            }
            Command::AddShape {
                page,
                shape,
                stroke,
                fill,
            } => Created::Object(tx.add_shape(page, &shape, stroke.as_ref(), fill)?),
            Command::RemoveObject(object) => tx.remove_object(object).map(|_| Created::Nothing)?,
        })
    }
}

/// Runs commands as one transaction: begin, apply each command, commit.
/// On any error, the transaction is dropped and nothing changes.
pub fn run_commands(
    document: &Doc,
    label: &str,
    commands: Vec<Command>,
) -> Result<TransactionId, ApiError> {
    let tx = edit::begin(document, label)?;
    for (index, command) in commands.into_iter().enumerate() {
        command.apply(&tx).map_err(|mut e| {
            e.command_index.get_or_insert(index as u32);
            e
        })?;
    }
    Transaction::commit(tx)
}
