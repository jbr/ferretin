use rustdoc_types::ItemKind;
use std::path::PathBuf;

use crate::{
    Navigator,
    sources::{LocalSource, StdSource},
};

fn get_fixture_crate_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixture-crate")
}

fn test_navigator() -> Navigator {
    Navigator::default()
        .with_local_source(LocalSource::load(&get_fixture_crate_path()).ok())
        .with_std_source(StdSource::from_rustup())
}

/// Resolve a path, panicking with a helpful message on failure.
fn resolve<'a>(nav: &'a Navigator, path: &str) -> crate::DocRef<'a, rustdoc_types::Item> {
    nav.resolve_path(path, &mut vec![])
        .unwrap_or_else(|| panic!("failed to resolve {path:?}"))
}

/// Check that `discriminated_path()` produces the expected string.
#[test]
fn discriminated_path_values() {
    let nav = test_navigator();
    // The crate name in discriminated_path uses crate_docs().name() which matches the
    // Cargo.toml name ("fixture-crate" with dashes, not underscores).
    let cases = [
        ("crate::TestStruct", "fixture-crate::struct@TestStruct"),
        ("crate::TestTrait", "fixture-crate::trait@TestTrait"),
        ("crate::test_function", "fixture-crate::fn@test_function"),
        ("crate::submodule", "fixture-crate::mod@submodule"),
        ("crate::TEST_CONSTANT", "fixture-crate::const@TEST_CONSTANT"),
        ("crate::TEST_STATIC", "fixture-crate::static@TEST_STATIC"),
        ("crate::GenericEnum", "fixture-crate::enum@GenericEnum"),
        (
            "crate::namespace_collisions",
            "fixture-crate::mod@namespace_collisions",
        ),
    ];

    for (path, expected_disc) in cases {
        let item = resolve(&nav, path);
        let disc = item
            .discriminated_path()
            .unwrap_or_else(|| panic!("no discriminated_path for {path:?}"));
        assert_eq!(disc, expected_disc, "wrong discriminated_path for {path:?}");
    }
}

/// Check that `discriminated_path()` → `resolve_path()` returns the same item.
#[test]
fn discriminated_path_round_trips() {
    let nav = test_navigator();
    let paths = [
        "crate::TestStruct",
        "crate::TestTrait",
        "crate::test_function",
        "crate::submodule",
        "crate::TEST_CONSTANT",
        "crate::GenericEnum",
        "crate::submodule::SubStruct",
        "crate::namespace_collisions",
    ];

    for path in paths {
        let item = resolve(&nav, path);
        let disc_path = item
            .discriminated_path()
            .unwrap_or_else(|| panic!("no discriminated_path for {path:?}"));
        let round_tripped = nav
            .resolve_path(&disc_path, &mut vec![])
            .unwrap_or_else(|| panic!("discriminated path {disc_path:?} failed to resolve"));
        assert_eq!(
            item, round_tripped,
            "round-trip mismatch for {path:?} (discriminated: {disc_path:?})"
        );
    }
}

/// Methods have no `ItemSummary` in rustdoc's `paths` map (rust-lang/rust#152511), so
/// `discriminated_path()` falls back to the `parent` set during tree traversal.
#[test]
fn discriminated_path_round_trips_method() {
    let nav = test_navigator();
    let item = resolve(&nav, "crate::submodule::SubStruct::new");
    let disc_path = item
        .discriminated_path()
        .expect("discriminated_path should work for methods once the upstream bug is fixed");
    let round_tripped = nav
        .resolve_path(&disc_path, &mut vec![])
        .unwrap_or_else(|| panic!("discriminated path {disc_path:?} failed to resolve"));
    assert_eq!(item, round_tripped);
}

/// A discriminator prefix selects the right item when a module and function share a name.
#[test]
fn discriminator_resolves_module_function_collision() {
    let nav = test_navigator();

    let by_mod = resolve(&nav, "crate::namespace_collisions::mod@both");
    let by_fn = resolve(&nav, "crate::namespace_collisions::fn@both");

    assert_eq!(
        by_mod.kind(),
        ItemKind::Module,
        "mod@both should be a module"
    );
    assert_eq!(
        by_fn.kind(),
        ItemKind::Function,
        "fn@both should be a function"
    );
    assert_ne!(
        by_mod, by_fn,
        "mod@both and fn@both should be different items"
    );
}

/// A discriminated path round-trips for both sides of a module-function collision.
#[test]
fn discriminated_path_round_trips_through_collision() {
    let nav = test_navigator();

    for disc_path in [
        "fixture-crate::namespace_collisions::mod@both",
        "fixture-crate::namespace_collisions::fn@both",
    ] {
        let item = nav
            .resolve_path(disc_path, &mut vec![])
            .unwrap_or_else(|| panic!("failed to resolve {disc_path:?}"));
        let generated = item
            .discriminated_path()
            .unwrap_or_else(|| panic!("no discriminated_path for {disc_path:?}"));
        assert_eq!(
            generated, disc_path,
            "discriminated_path should reproduce the qualified path"
        );
    }
}

/// A method on a struct in a private module round-trips through `discriminated_path`.
///
/// This is the hardest combined case: the method is absent from rustdoc's `paths` map
/// (rust-lang/rust#152511), and its parent struct's `ItemSummary::path` passes through
/// a private module, so tree traversal alone cannot anchor the parent either.
/// Resolution must use the path_to_id index to find the parent, then traverse into
/// the method via `find_children_recursive`.
#[test]
fn discriminated_path_round_trips_method_on_private_module_struct() {
    let nav = test_navigator();

    // Resolve via the public re-export path to get a DocRef with parent set.
    let struct_item = resolve(&nav, "crate::ReachableViaPrivateModule");
    let method = struct_item
        .child_items()
        .find(|c| c.name() == Some("private_module_method"))
        .expect("private_module_method not found");

    let disc = method
        .discriminated_path()
        .expect("discriminated_path should work: parent was set during child_items traversal");

    // The path goes through the private module because that's where ItemSummary::path points.
    assert_eq!(
        disc,
        "fixture-crate::private_detail::ReachableViaPrivateModule::fn@private_module_method"
    );

    let round_tripped = nav
        .resolve_path(&disc, &mut vec![])
        .unwrap_or_else(|| panic!("failed to resolve discriminated path {disc:?}"));

    assert_eq!(method, round_tripped);
}

/// Items that live behind a private module are reachable via the path_to_id fallback.
#[test]
fn private_module_path_resolves_via_index() {
    let nav = test_navigator();

    // The public re-export is reachable via normal tree traversal.
    let via_reexport = resolve(&nav, "crate::ReachableViaPrivateModule");

    // The path through the private module fails tree traversal but succeeds via path_to_id.
    let via_private_path = resolve(
        &nav,
        "fixture-crate::private_detail::ReachableViaPrivateModule",
    );

    assert_eq!(via_reexport.kind(), ItemKind::Struct, "should be a struct");
    // Both paths should land on the same underlying item.
    assert_eq!(
        via_reexport, via_private_path,
        "re-export and private-module path should resolve to the same item"
    );
}
