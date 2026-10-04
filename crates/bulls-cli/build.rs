// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=i18n/manifest.json");
    println!("cargo:rerun-if-changed=i18n/messages.json");
    println!("cargo:rerun-if-changed=i18n/locales");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    );
    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR for build scripts"),
    );
    let i18n_root = manifest_dir.join("i18n");
    let output = out_dir.join("bulls_i18n.rs");

    if let Err(error) = bulls_i18n::compile(&i18n_root, &output) {
        panic!("BullSaddle i18n compilation failed: {error}");
    }
}
