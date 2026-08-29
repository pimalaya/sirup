use std::env;

use pimalaya_cli::build::{features_env, git_envs, target_envs};

fn main() {
    features_env(include_str!("./Cargo.toml"));
    target_envs();
    git_envs();

    // `discovery` collapses the feature list the wizard's discovery
    // probes need: both protocols, since one input may resolve to
    // either, and a TLS provider, every `.well-known` endpoint being
    // HTTPS. Cargo exports `CARGO_FEATURE_<NAME>` for every enabled
    // feature.
    println!("cargo::rustc-check-cfg=cfg(discovery)");

    let enabled = |feature: &str| env::var_os(format!("CARGO_FEATURE_{feature}")).is_some();
    let protocols = enabled("IMAP") && enabled("SMTP");
    let tls = ["RUSTLS_RING", "RUSTLS_AWS", "NATIVE_TLS"]
        .iter()
        .any(|feature| enabled(feature));

    if protocols && tls {
        println!("cargo::rustc-cfg=discovery");
    }
}
