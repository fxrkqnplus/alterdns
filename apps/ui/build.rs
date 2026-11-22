// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() -> std::io::Result<()> {
    alterdns_buildinfo::embed(&alterdns_buildinfo::ExeInfo {
        file_description: "AlterDNS",
        original_filename: "alterdns-ui.exe",
        icon: Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/brand/generated/app.ico"
        )),
        manifest: Some(alterdns_buildinfo::GUI_MANIFEST),
    })
}
