// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() -> std::io::Result<()> {
    alterdns_buildinfo::embed(&alterdns_buildinfo::ExeInfo {
        file_description: "AlterDNS bakım",
        original_filename: "alterdns-maint.exe",
    })
}
