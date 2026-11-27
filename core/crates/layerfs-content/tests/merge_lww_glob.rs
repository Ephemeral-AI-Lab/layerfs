use layerfs_content::merge::{Rule, Rules};

#[test]
fn recursive_directory_glob_includes_direct_children() {
    let rule = Rule::glob("test/**/*.test.js");
    for path in [
        "/test/cart.test.js",
        "/test/routes/cart-api.test.js",
        "/test/a/b/deep.test.js",
    ] {
        assert!(rule.matches_path(path), "{path}");
    }
    for path in [
        "/other/cart.test.js",
        "/test/routes/cart.js",
        "/test/prefixcart.test.jsx",
    ] {
        assert!(!rule.matches_path(path), "{path}");
    }
}

#[test]
fn recursive_directory_glob_requires_complete_directory_names() {
    let rule = Rule::glob("src/**/index.js");
    assert!(rule.matches_path("/src/index.js"));
    assert!(rule.matches_path("/src/web/index.js"));
    assert!(!rule.matches_path("/src/webindex.js"));
    assert!(Rule::glob("**/index.js").matches_path("/index.js"));
    assert!(Rule::glob("src/**/*.js").matches_path("/src/cart.js"));
    assert!(!Rule::glob("src/*.js").matches_path("/src/web/cart.js"));
}

#[test]
fn last_writer_rules_share_recursive_directory_semantics() {
    let rules =
        Rules::load(&mut |path| Ok((path == "/.lww").then(|| "dist/**/bundle.js\n".into())))
            .unwrap();
    assert!(rules.matching("/dist/bundle.js").is_some());
    assert!(rules.matching("/dist/web/bundle.js").is_some());
    assert!(rules.matching("/dist/webbundle.js").is_none());
}

#[test]
fn consecutive_recursive_directory_globs_include_zero_directories() {
    for pattern in ["src/**/**/index.js", "src/**/**/**/index.js"] {
        let rule = Rule::glob(pattern);
        for path in ["/src/index.js", "/src/web/index.js", "/src/web/ui/index.js"] {
            assert!(rule.matches_path(path), "{pattern}: {path}");
        }
        assert!(!rule.matches_path("/src/webindex.js"), "{pattern}");
    }
    assert!(Rule::glob("**/**/index.js").matches_path("/index.js"));
    let rules =
        Rules::load(&mut |path| Ok((path == "/.lww").then(|| "dist/**/**/bundle.js\n".into())))
            .unwrap();
    assert!(rules.matching("/dist/bundle.js").is_some());
    assert!(rules.matching("/dist/web/bundle.js").is_some());
    assert!(rules.matching("/dist/webbundle.js").is_none());
}
