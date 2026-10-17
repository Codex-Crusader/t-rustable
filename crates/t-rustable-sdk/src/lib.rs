//! Rust bindings and helpers for T-Rustable mods.
//!
//! A mod implements [`Tool`] (GUI), [`BatchJob`] (CLI), or both, and exports them with
//! [`export_interactive!`], [`export_batch!`], or [`export_full!`].
//! Build the mod for `wasm32-unknown-unknown`, then run `wasm-tools component new`.
//!
//! The `manifest` and `permissions` modules have no WIT dependency. The host uses them too.

pub mod manifest;
pub mod permissions;

#[cfg(feature = "guest")]
pub mod bindings;
#[cfg(feature = "guest")]
mod command;
#[cfg(feature = "guest")]
mod error;
#[cfg(feature = "guest")]
mod geometry;
#[cfg(feature = "guest")]
mod ids;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version of the WIT package `t-rustable:api` that this SDK targets.
pub const API_VERSION: &str = "0.1.0";

#[cfg(feature = "guest")]
pub use bindings::interactive::t_rustable::api::{document, edit, files, host, types, ui};
#[cfg(feature = "guest")]
pub use command::{Command, Created, run_commands};
#[cfg(feature = "guest")]
pub use error::{api_error, parse_args};

/// Types of the exported interfaces.
#[cfg(feature = "guest")]
pub mod exports {
    pub use crate::bindings::batch::exports::t_rustable::api::batch_job;
    pub use crate::bindings::interactive::exports::t_rustable::api::tool;
}

#[cfg(feature = "guest")]
pub use guest::{BatchJob, Tool};

#[cfg(feature = "guest")]
mod guest {
    use crate::document::Doc;
    use crate::exports::tool::{
        Cursor, DocumentChange, KeyEvent, KeyResult, PointerEvent, PointerResult,
    };
    use crate::types::ApiError;

    /// An interactive mod. Implement only the methods that you need.
    pub trait Tool {
        /// The user selected this tool.
        fn activate() -> Result<(), ApiError> {
            Ok(())
        }

        /// The user selected another tool.
        fn deactivate() {}

        /// The user ran an action from the manifest. `args` is a JSON string.
        fn run_action(
            document: Option<&Doc>,
            action_id: String,
            args: String,
        ) -> Result<(), ApiError> {
            let _ = (document, args);
            Err(crate::api_error(
                "unknown-action",
                format!("This mod has no action {action_id:?}."),
            ))
        }

        /// Pointer event in page coordinates, while this tool is active.
        fn on_pointer(document: &Doc, event: PointerEvent) -> PointerResult {
            let _ = (document, event);
            PointerResult {
                overlay: Vec::new(),
                cursor: Cursor::Default,
                handled: false,
            }
        }

        /// Key event, while this tool is active.
        fn on_key(document: &Doc, event: KeyEvent) -> KeyResult {
            let _ = (document, event);
            KeyResult { handled: false }
        }

        /// Text that the user typed, after input method composition, while this tool is active.
        fn on_text_input(document: &Doc, text: String) {
            let _ = (document, text);
        }

        /// JSON message from a panel of this mod.
        fn on_panel_message(panel_id: String, message: String) -> Result<(), ApiError> {
            let _ = (panel_id, message);
            Ok(())
        }

        /// The document changed, from any source.
        fn on_document_changed(document: &Doc, change: DocumentChange) {
            let _ = (document, change);
        }
    }

    /// A batch mod for the command line.
    pub trait BatchJob {
        /// `args` and the result are JSON strings.
        fn run(document: &Doc, args: String) -> Result<String, ApiError>;
    }
}

/// Exports a [`Tool`] as an interactive mod.
#[cfg(feature = "guest")]
#[macro_export]
macro_rules! export_interactive {
    ($ty:ty) => {
        const _: () = {
            struct __TRustableTool;

            impl $crate::exports::tool::Guest for __TRustableTool {
                fn activate() -> ::core::result::Result<(), $crate::types::ApiError> {
                    <$ty as $crate::Tool>::activate()
                }
                fn deactivate() {
                    <$ty as $crate::Tool>::deactivate()
                }
                fn run_action(
                    document: ::core::option::Option<&$crate::document::Doc>,
                    action_id: ::std::string::String,
                    args: ::std::string::String,
                ) -> ::core::result::Result<(), $crate::types::ApiError> {
                    <$ty as $crate::Tool>::run_action(document, action_id, args)
                }
                fn on_pointer(
                    document: &$crate::document::Doc,
                    event: $crate::exports::tool::PointerEvent,
                ) -> $crate::exports::tool::PointerResult {
                    <$ty as $crate::Tool>::on_pointer(document, event)
                }
                fn on_text_input(document: &$crate::document::Doc, text: ::std::string::String) {
                    <$ty as $crate::Tool>::on_text_input(document, text)
                }
                fn on_key(
                    document: &$crate::document::Doc,
                    event: $crate::exports::tool::KeyEvent,
                ) -> $crate::exports::tool::KeyResult {
                    <$ty as $crate::Tool>::on_key(document, event)
                }
                fn on_panel_message(
                    panel_id: ::std::string::String,
                    message: ::std::string::String,
                ) -> ::core::result::Result<(), $crate::types::ApiError> {
                    <$ty as $crate::Tool>::on_panel_message(panel_id, message)
                }
                fn on_document_changed(
                    document: &$crate::document::Doc,
                    change: $crate::exports::tool::DocumentChange,
                ) {
                    <$ty as $crate::Tool>::on_document_changed(document, change)
                }
            }

            $crate::bindings::interactive::export_interactive_world!(
                __TRustableTool with_types_in $crate::bindings::interactive
            );
        };
    };
}

/// Exports a [`BatchJob`] as a batch mod.
#[cfg(feature = "guest")]
#[macro_export]
macro_rules! export_batch {
    ($ty:ty) => {
        const _: () = {
            struct __TRustableBatch;

            impl $crate::exports::batch_job::Guest for __TRustableBatch {
                fn run(
                    document: &$crate::document::Doc,
                    args: ::std::string::String,
                ) -> ::core::result::Result<::std::string::String, $crate::types::ApiError>
                {
                    <$ty as $crate::BatchJob>::run(document, args)
                }
            }

            $crate::bindings::batch::export_batch_world!(
                __TRustableBatch with_types_in $crate::bindings::batch
            );
        };
    };
}

/// Exports a type that implements both [`Tool`] and [`BatchJob`].
/// The component then targets the `full` world.
#[cfg(feature = "guest")]
#[macro_export]
macro_rules! export_full {
    ($ty:ty) => {
        $crate::export_interactive!($ty);
        $crate::export_batch!($ty);
    };
}
