use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::Hash;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use super::fixtures::{gen_ident, gen_map, repeat_until};
use super::{Key, MmapHashMap, UniversalHashMap, serialize_hashmap};
use crate::universal_io::{self, MmapFile};

#[test]
fn test_mmap_hash() {
    test_mmap_hash_impl(gen_ident, |s| s.as_str(), |s| s.to_owned());
    test_mmap_hash_impl(|rng| rng.random::<i64>(), |i| i, |i| *i);
    test_mmap_hash_impl(|rng| rng.random::<u128>(), |i| i, |i| *i);
}

fn test_mmap_hash_impl<K: Key + ?Sized, K1: Ord + Hash>(
    generator: impl Clone + Fn(&mut StdRng) -> K1,
    as_ref: impl Fn(&K1) -> &K,
    from_ref: impl Fn(&K) -> K1,
) {
    let mut rng = StdRng::seed_from_u64(42);
    let tmpdir = tempfile::Builder::new().tempdir().unwrap();

    let map = gen_map(&mut rng, generator.clone(), 1000);
    serialize_hashmap(
        &tmpdir.path().join("map"),
        map.iter().map(|(k, v)| (as_ref(k), v.iter().copied())),
    )
    .unwrap();
    let mmap = MmapHashMap::<K, u32>::open(&tmpdir.path().join("map"), false).unwrap();

    // Non-existing keys should return None
    for _ in 0..1000 {
        let key = repeat_until(|| generator(&mut rng), |key| !map.contains_key(key));
        assert!(mmap.get(as_ref(&key)).unwrap().is_none());
    }

    // check keys iterator
    for key in mmap.keys() {
        let key = from_ref(key);
        assert!(map.contains_key(&key));
    }
    assert_eq!(mmap.keys_count(), map.len());
    assert_eq!(mmap.keys().count(), map.len());

    for (k, v) in mmap.iter() {
        let v = v.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(map.get(&from_ref(k)).unwrap(), &v);
    }

    let keys: Vec<_> = mmap.keys().collect();
    assert_eq!(keys.len(), map.len());

    // Existing keys should return the correct values
    for (k, v) in map {
        assert_eq!(
            mmap.get(as_ref(&k)).unwrap().unwrap(),
            &v.into_iter().collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_mmap_hash_impl_u64_value() {
    let mut rng = StdRng::seed_from_u64(42);
    let tmpdir = tempfile::Builder::new().tempdir().unwrap();

    let mut map: HashMap<i64, BTreeSet<u64>> = Default::default();

    for key in 0..10i64 {
        map.insert(key, (0..100).map(|_| rng.random_range(0..=1000)).collect());
    }

    serialize_hashmap(
        &tmpdir.path().join("map"),
        map.iter().map(|(k, v)| (k, v.iter().copied())),
    )
    .unwrap();

    let mmap = MmapHashMap::<i64, u64>::open(&tmpdir.path().join("map"), true).unwrap();

    for (k, v) in map {
        assert_eq!(
            mmap.get(&k).unwrap().unwrap(),
            &v.into_iter().collect::<Vec<_>>()
        );
    }

    assert!(mmap.get(&100).unwrap().is_none())
}

#[test]
fn test_mmap_hash_impl_u128_value() {
    let mut rng = StdRng::seed_from_u64(42);
    let tmpdir = tempfile::Builder::new().tempdir().unwrap();

    let mut map: HashMap<u128, BTreeSet<u32>> = Default::default();

    map.insert(
        9812384971724u128,
        (0..100).map(|_| rng.random_range(0..=1000)).collect(),
    );

    serialize_hashmap(
        &tmpdir.path().join("map"),
        map.iter().map(|(k, v)| (k, v.iter().copied())),
    )
    .unwrap();

    let mmap = MmapHashMap::<u128, u32>::open(&tmpdir.path().join("map"), true).unwrap();

    let keys: Vec<_> = mmap.keys().collect();
    assert_eq!(keys.len(), map.len());

    for (k, v) in map {
        assert_eq!(
            mmap.get(&k).unwrap().unwrap(),
            &v.into_iter().collect::<Vec<_>>()
        );
    }
    assert!(mmap.get(&100).unwrap().is_none())
}

// =========================== UniversalHashMap tests ==========================

type UMap<K, V> = UniversalHashMap<K, V, MmapFile>;

/// Helper: create a file with MmapHashMap, open it with UniversalHashMap.
fn write_and_open<K, V, K1>(
    map: &BTreeMap<K1, BTreeSet<V>>,
    as_ref: impl Fn(&K1) -> &K,
) -> (tempfile::TempDir, UMap<K, V>)
where
    K: Key + ?Sized,
    V: Sized + Copy + FromBytes + Immutable + IntoBytes + KnownLayout,
    K1: Ord + Hash,
{
    let tmpdir = tempfile::Builder::new().tempdir().unwrap();
    let path = tmpdir.path().join("map");
    serialize_hashmap(
        &path,
        map.iter().map(|(k, v)| (as_ref(k), v.iter().copied())),
    )
    .unwrap();
    let umap = UMap::<K, V>::open(&path, Default::default()).unwrap();
    (tmpdir, umap)
}

fn test_get_impl<K: Key + ?Sized, K1: Ord + Hash>(
    generator: impl Clone + Fn(&mut StdRng) -> K1,
    as_ref: impl Fn(&K1) -> &K,
    count: usize,
) {
    let mut rng = StdRng::seed_from_u64(42);
    let map = gen_map(&mut rng, generator.clone(), count);
    let (_tmpdir, umap) = write_and_open(&map, &as_ref);

    for (k, v) in &map {
        let got = umap.get(as_ref(k)).unwrap().unwrap();
        let expected: Vec<u32> = v.iter().copied().collect();
        assert_eq!(got, expected);
    }

    for _ in 0..100 {
        let key = repeat_until(|| generator(&mut rng), |key| !map.contains_key(key));
        assert!(umap.get(as_ref(&key)).unwrap().is_none());
    }
}

#[test]
fn test_get_str_keys() {
    test_get_impl(gen_ident, |s| s.as_str(), 200);
}

#[test]
fn test_get_i64_keys() {
    test_get_impl(|rng| rng.random::<i64>(), |i| i, 200);
}

#[test]
fn test_get_u128_keys() {
    test_get_impl(|rng| rng.random::<u128>(), |i| i, 200);
}

// ── get_values_count ───────────────────────────────────────────────────

fn test_get_values_count_impl<K: Key + ?Sized, K1: Ord + Hash>(
    generator: impl Clone + Fn(&mut StdRng) -> K1,
    as_ref: impl Fn(&K1) -> &K,
) {
    let mut rng = StdRng::seed_from_u64(77);
    let map = gen_map(&mut rng, generator.clone(), 150);
    let (_tmpdir, umap) = write_and_open(&map, &as_ref);

    for (k, v) in &map {
        let count = umap.get_values_count(as_ref(k)).unwrap().unwrap();
        assert_eq!(count, v.len());
    }

    let missing = repeat_until(|| generator(&mut rng), |key| !map.contains_key(key));
    assert!(umap.get_values_count(as_ref(&missing)).unwrap().is_none());
}

#[test]
fn test_get_values_count_str() {
    test_get_values_count_impl(gen_ident, |s| s.as_str());
}

#[test]
fn test_get_values_count_i64() {
    test_get_values_count_impl(|rng| rng.random::<i64>(), |i| i);
}

#[test]
fn test_get_values_count_u128() {
    test_get_values_count_impl(|rng| rng.random::<u128>(), |i| i);
}

// ── for_each_entry ─────────────────────────────────────────────────────

fn test_for_each_entry_impl<K: Key + ?Sized, K1: Ord + Hash + Clone>(
    generator: impl Clone + Fn(&mut StdRng) -> K1,
    as_ref: impl Fn(&K1) -> &K,
    from_ref: impl Fn(&K) -> K1,
    count: usize,
) {
    let mut rng = StdRng::seed_from_u64(55);
    let map = gen_map(&mut rng, generator, count);
    let (_tmpdir, umap) = write_and_open(&map, &as_ref);

    let mut visited: BTreeMap<K1, Vec<u32>> = BTreeMap::new();
    umap.for_each_entry(|k, vals| -> universal_io::Result<()> {
        visited.insert(from_ref(k), vals.to_vec());
        Ok(())
    })
    .unwrap();

    assert_eq!(visited.len(), map.len());
    for (k, v) in &map {
        let expected: Vec<u32> = v.iter().copied().collect();
        assert_eq!(visited.get(k).unwrap(), &expected);
    }
}

#[test]
fn test_for_each_entry_str() {
    test_for_each_entry_impl(gen_ident, |s| s.as_str(), |s| s.to_owned(), 200);
}

#[test]
fn test_for_each_entry_i64() {
    test_for_each_entry_impl(|rng| rng.random::<i64>(), |i| i, |i| *i, 200);
}

#[test]
fn test_for_each_entry_u128() {
    test_for_each_entry_impl(|rng| rng.random::<u128>(), |i| i, |i| *i, 200);
}

// ── for_each_key ───────────────────────────────────────────────────────

fn test_for_each_key_impl<K: Key + ?Sized, K1: Ord + Hash + Clone + std::fmt::Debug>(
    generator: impl Clone + Fn(&mut StdRng) -> K1,
    as_ref: impl Fn(&K1) -> &K,
    from_ref: impl Fn(&K) -> K1,
    count: usize,
) {
    let mut rng = StdRng::seed_from_u64(33);
    let map = gen_map(&mut rng, generator, count);
    let (_tmpdir, umap) = write_and_open(&map, &as_ref);

    let mut keys: BTreeSet<K1> = BTreeSet::new();
    umap.for_each_key(|k| -> universal_io::Result<()> {
        keys.insert(from_ref(k));
        Ok(())
    })
    .unwrap();

    let expected_keys: BTreeSet<K1> = map.keys().cloned().collect();
    assert_eq!(keys, expected_keys);
}

#[test]
fn test_for_each_key_str() {
    test_for_each_key_impl(gen_ident, |s| s.as_str(), |s| s.to_owned(), 200);
}

#[test]
fn test_for_each_key_i64() {
    test_for_each_key_impl(|rng| rng.random::<i64>(), |i| i, |i| *i, 200);
}

#[test]
fn test_for_each_key_u128() {
    test_for_each_key_impl(|rng| rng.random::<u128>(), |i| i, |i| *i, 200);
}

// ── keys_count ─────────────────────────────────────────────────────────

#[test]
fn test_keys_count() {
    let mut rng = StdRng::seed_from_u64(11);
    for count in [0, 1, 10, 500] {
        let map = gen_map(&mut rng, |rng| rng.random::<i64>(), count);
        let (_tmpdir, umap) = write_and_open::<i64, u32, _>(&map, |k| k);
        assert_eq!(umap.keys_count(), count);
    }
}

// ── Different value sizes ──────────────────────────────────────────────

#[test]
fn test_u64_values() {
    let mut rng = StdRng::seed_from_u64(88);

    let mut map: BTreeMap<i64, BTreeSet<u64>> = BTreeMap::new();
    for key in 0..50i64 {
        map.insert(
            key,
            (0..rng.random_range(1..=100))
                .map(|_| rng.random())
                .collect(),
        );
    }

    let (_tmpdir, umap) = write_and_open::<i64, u64, _>(&map, |k| k);

    for (k, v) in &map {
        let got = umap.get(k).unwrap().unwrap();
        let expected: Vec<u64> = v.iter().copied().collect();
        assert_eq!(got, expected);
    }

    assert!(umap.get(&9999).unwrap().is_none());
}

#[test]
fn test_u128_values() {
    let mut rng = StdRng::seed_from_u64(77);

    let mut map: BTreeMap<u128, BTreeSet<u128>> = BTreeMap::new();
    for _ in 0..30 {
        let key = rng.random::<u128>();
        map.insert(
            key,
            (0..rng.random_range(1..=50))
                .map(|_| rng.random())
                .collect(),
        );
    }

    let (_tmpdir, umap) = write_and_open::<u128, u128, _>(&map, |k| k);

    for (k, v) in &map {
        let got = umap.get(k).unwrap().unwrap();
        let expected: Vec<u128> = v.iter().copied().collect();
        assert_eq!(got, expected);
    }
}

// ── Edge: single value per key ─────────────────────────────────────────

#[test]
fn test_single_value_per_key() {
    let mut map: BTreeMap<i64, BTreeSet<u32>> = BTreeMap::new();
    for i in 0..100 {
        let mut s = BTreeSet::new();
        s.insert(i as u32);
        map.insert(i, s);
    }

    let (_tmpdir, umap) = write_and_open::<i64, u32, _>(&map, |k| k);

    for (k, v) in &map {
        let got = umap.get(k).unwrap().unwrap();
        assert_eq!(got, vec![*v.iter().next().unwrap()]);
        assert_eq!(umap.get_values_count(k).unwrap().unwrap(), 1);
    }
}

// ── Edge: many values per key ──────────────────────────────────────────

#[test]
fn test_many_values_per_key() {
    let mut rng = StdRng::seed_from_u64(22);

    let mut map: BTreeMap<i64, BTreeSet<u32>> = BTreeMap::new();
    for i in 0..10 {
        map.insert(
            i,
            (0..1000).map(|_| rng.random_range(0..=100_000)).collect(),
        );
    }

    let (_tmpdir, umap) = write_and_open::<i64, u32, _>(&map, |k| k);

    for (k, v) in &map {
        let got = umap.get(k).unwrap().unwrap();
        let expected: Vec<u32> = v.iter().copied().collect();
        assert_eq!(got, expected);
        assert_eq!(umap.get_values_count(k).unwrap().unwrap(), v.len());
    }
}

// ── Edge: empty map ────────────────────────────────────────────────────

#[test]
fn test_empty_map() {
    let map: BTreeMap<i64, BTreeSet<u32>> = BTreeMap::new();
    let (_tmpdir, umap) = write_and_open::<i64, u32, _>(&map, |k| k);

    assert_eq!(umap.keys_count(), 0);
    assert!(umap.get(&0).unwrap().is_none());
    assert!(umap.get_values_count(&0).unwrap().is_none());

    let mut count = 0;
    umap.for_each_entry(|_, _| -> universal_io::Result<()> {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 0);

    umap.for_each_key(|_: &i64| -> universal_io::Result<()> {
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 0);
}

// ── Edge: long string keys ─────────────────────────────────────────────

#[test]
fn test_long_string_keys() {
    let mut map: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for i in 0..20 {
        // Keys longer than KEY_READ_CAP (512) to exercise the retry path in for_each_key.
        let key: String = std::iter::repeat_n('x', 600 + i).collect();
        let mut s = BTreeSet::new();
        s.insert(i as u32);
        map.insert(key, s);
    }

    let (_tmpdir, umap) = write_and_open::<str, u32, _>(&map, |k| k.as_str());

    for (k, v) in &map {
        let got = umap.get(k.as_str()).unwrap().unwrap();
        let expected: Vec<u32> = v.iter().copied().collect();
        assert_eq!(got, expected);
    }

    let mut key_count = 0;
    umap.for_each_key(|_| -> universal_io::Result<()> {
        key_count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(key_count, map.len());

    let mut entry_count = 0;
    umap.for_each_entry(|k, _| -> universal_io::Result<()> {
        assert!(map.contains_key(k));
        entry_count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(entry_count, map.len());
}

// ── Batch boundary: entry count > ENTRY_BATCH_SIZE ─────────────────────

#[test]
fn test_for_each_entry_exceeds_batch_size() {
    // ENTRY_BATCH_SIZE is 64 inside for_each_entry — use more entries to
    // exercise multi-batch iteration.
    let mut rng = StdRng::seed_from_u64(44);
    let map = gen_map(&mut rng, |rng| rng.random::<i64>(), 300);
    let (_tmpdir, umap) = write_and_open::<i64, u32, _>(&map, |k| k);

    let mut visited: BTreeMap<i64, Vec<u32>> = BTreeMap::new();
    umap.for_each_entry(|k, vals| -> universal_io::Result<()> {
        visited.insert(*k, vals.to_vec());
        Ok(())
    })
    .unwrap();

    assert_eq!(visited.len(), map.len());
    for (k, v) in &map {
        let expected: Vec<u32> = v.iter().copied().collect();
        assert_eq!(visited[k], expected);
    }
}
