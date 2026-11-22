// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Katalog kapsamı (T-ERR-003): geçerli faza kadarki her hata kodu ürün kodunun testlerinde
//! üretilir. 1E'den itibaren `xtask check` bunu katı kipte (hata olarak) uygular.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtask::check::{repo_files, repo_root};
use xtask::docs_errors::{CATALOG_COVERAGE_STRICT, coverage};

#[test]
fn coverage_is_strict_and_complete() {
    assert!(CATALOG_COVERAGE_STRICT, "1E'de katı kipe geçilir");
    let root = repo_root().unwrap();
    coverage(&root, &repo_files(&root).unwrap()).unwrap();
}
