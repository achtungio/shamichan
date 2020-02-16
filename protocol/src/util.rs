use std::collections::{HashMap, HashSet};
use std::hash::Hash;

// Maps of K to sets of V
#[derive(Clone)]
pub struct SetMap<K, V>(HashMap<K, HashSet<V>>)
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone;

impl<K, V> Default for SetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	fn default() -> Self {
		Self(HashMap::new())
	}
}

impl<K, V> SetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	pub fn insert(&mut self, k: K, v: V) {
		self.0.entry(k).or_default().insert(v);
	}

	pub fn remove(&mut self, k: &K, v: &V) {
		if let Some(set) = self.0.get_mut(k) {
			set.remove(v);
			if set.len() == 0 {
				self.0.remove(k);
			}
		}
	}

	pub fn get(&self, k: &K) -> Option<&HashSet<V>> {
		self.0.get(k)
	}

	pub fn drain(
		&mut self,
	) -> std::collections::hash_map::Drain<'_, K, HashSet<V>> {
		self.0.drain()
	}
}

impl<K, V> std::iter::FromIterator<(K, HashSet<V>)> for SetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	fn from_iter<T: IntoIterator<Item = (K, HashSet<V>)>>(iter: T) -> Self {
		Self(HashMap::<K, HashSet<V>>::from_iter(iter))
	}
}

// Maps of K to sets of V and V to sets of K simultaneously
pub struct DoubleSetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	by_key: SetMap<K, V>,
	by_value: SetMap<V, K>,
}

impl<K, V> Default for DoubleSetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	fn default() -> Self {
		Self {
			by_key: Default::default(),
			by_value: Default::default(),
		}
	}
}

impl<K, V> DoubleSetMap<K, V>
where
	K: Hash + Eq + Clone,
	V: Hash + Eq + Clone,
{
	pub fn insert(&mut self, k: K, v: V) {
		self.by_key.insert(k.clone(), v.clone());
		self.by_value.insert(v, k);
	}

	pub fn get_by_key(&self, k: &K) -> Option<&HashSet<V>> {
		self.by_key.get(k)
	}

	pub fn get_by_value(&self, v: &V) -> Option<&HashSet<K>> {
		self.by_value.get(v)
	}

	pub fn remove_by_key(&mut self, k: &K) {
		if let Some(set) = self.by_key.0.remove(&k) {
			for v in set {
				self.by_value.remove(&v, &k);
			}
		}
	}

	pub fn remove_by_value(&mut self, v: &V) {
		if let Some(set) = self.by_value.0.remove(&v) {
			for k in set {
				self.by_key.remove(&k, &v);
			}
		}
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! _debug_log_inner {
	($arg:expr) => {
		eprintln!("{}", &$arg);
	};
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! _debug_log_inner {
	($arg:expr) => {{
		use wasm_bindgen::prelude::*;

		web_sys::console::log_1(&JsValue::from(&format!("{}", $arg)));
		}};
}

#[macro_export]
macro_rules! debug_log {
    ($arg:expr) => {
        if cfg!(debug_assertions) {
            $crate::_debug_log_inner!($arg);
        }
    };
	($label:expr, $arg:expr) => {
        debug_log!(format!("{}: {:?}", $label, &$arg));
    };
	($label:expr, $arg:expr, $($more:expr),+) => {
        debug_log!("{}: {:?}", $label, (&$arg $(, &$more)+));
	};
}