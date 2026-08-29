//! # JSON Schema registry
//!
//! Maps a CLI-invocation key, the command path joined with hyphens and
//! prefixed `sirup-`, to the JSON Schema of that command's `--json`
//! payload. [`JsonSchemaCommand`] writes one file per entry.
//!
//! Only the commands returning data appear: `start` and `repl` are
//! daemons, and completions and man pages are pimalaya-cli's own.
//!
//! [`JsonSchemaCommand`]: pimalaya_cli::clap::commands::JsonSchemaCommand

use std::collections::BTreeMap;

use schemars::schema_for;
use serde_json::Value;

use crate::wizard::configure::ConfigureOutput;

/// Builds the command-to-schema map consumed by `json-schema`.
pub fn schemas() -> BTreeMap<String, Value> {
    let schema = schema_for!(ConfigureOutput);

    BTreeMap::from([(
        String::from("sirup-configure"),
        serde_json::to_value(schema).expect("JSON Schema must serialize"),
    )])
}
