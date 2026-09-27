//! Checks that the [`MiMalloc`] allocator works with the `hashbrown` crate.
//!
//! Implicitly ensures that the versions of `allocator-api2` match.
#![no_std]

use mimalloc_rich::MiMalloc;

type MiHashMap<K, V> = hashbrown::HashMap<K, V, hashbrown::DefaultHashBuilder, mimalloc_rich::MiMalloc>;

#[test]
fn basic() {
    type ExampleMap = MiHashMap<&'static str, i32>;
    let mut res = ExampleMap::with_hasher_in(hashbrown::DefaultHashBuilder::default(), MiMalloc);
    res.insert("foo", 3);
    res.insert("bar", 7);
    assert_eq!(res["foo"], 3);
}
