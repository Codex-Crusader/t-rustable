//! Generated WIT bindings. The `full` world is both export macros together:
//! wit-component merges the two world sections into one component.
#![allow(unsafe_code, missing_docs, clippy::all)]

pub mod interactive {
    wit_bindgen::generate!({
        world: "interactive",
        path: "../../wit",
        pub_export_macro: true,
        export_macro_name: "export_interactive_world",
        default_bindings_module: "t_rustable_sdk::bindings::interactive",
        generate_all,
    });
}

pub mod batch {
    wit_bindgen::generate!({
        world: "batch",
        path: "../../wit",
        pub_export_macro: true,
        export_macro_name: "export_batch_world",
        default_bindings_module: "t_rustable_sdk::bindings::batch",
        with: {
            "t-rustable:api/types@0.1.1": crate::bindings::interactive::t_rustable::api::types,
            "t-rustable:api/document@0.1.1": crate::bindings::interactive::t_rustable::api::document,
            "t-rustable:api/edit@0.1.1": crate::bindings::interactive::t_rustable::api::edit,
            "t-rustable:api/host@0.1.1": crate::bindings::interactive::t_rustable::api::host,
        },
    });
}
