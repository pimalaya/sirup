//! # JSON Schema registry
//!
//! Maps a CLI-invocation key, the command path joined with hyphens and
//! prefixed `sirup-`, to the JSON Schema of that command's `--json`
//! payload. [`JsonSchemaCommand`] writes one file per entry. The
//! `sirup-error` key describes the failure object instead.
//!
//! No command returns data: `start` and `repl` are daemons, and
//! completions and man pages are pimalaya-cli's own, so the failure object
//! is the one entry.
//!
//! [`JsonSchemaCommand`]: pimalaya_cli::clap::commands::json_schema::JsonSchemaCommand

use std::collections::BTreeMap;

use pimalaya_cli::{clap::commands::json_schema::JsonSchemaRegistry, error::ErrorReport};
use schemars::schema_for;

/// Builds the command-to-schema registry consumed by `json-schema`.
///
/// Each value describes the type the command hands to the printer, and
/// `sirup-error` the failure object every command prints instead.
pub fn registry() -> JsonSchemaRegistry {
    let mut schemas = BTreeMap::new();

    macro_rules! insert {
        ($key:expr, $ty:ty) => {
            schemas.insert(
                $key.to_string(),
                serde_json::to_value(schema_for!($ty)).expect("JSON Schema must serialize"),
            );
        };
    }

    insert!("sirup-error", ErrorReport);

    JsonSchemaRegistry {
        bin: "sirup",
        version: env!("CARGO_PKG_VERSION"),
        schemas,
        events: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_payload_is_an_object() {
        assert_eq!(super::registry().non_object_roots(), Vec::<&str>::new());
    }
}
