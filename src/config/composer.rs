//! The shapes shared with Composer: `autoload` objects, and the parts of `composer.json` used to
//! infer settings.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

/// An `autoload` object, as in `composer.json`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Autoload {
    #[serde(rename = "psr-4", default, skip_serializing_if = "Ordered::is_empty")]
    pub psr4: Ordered<StringOrList>,
    #[serde(rename = "psr-0", default, skip_serializing_if = "Ordered::is_empty")]
    pub psr0: Ordered<StringOrList>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classmap: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(
        rename = "exclude-from-classmap",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub exclude_from_classmap: Vec<String>,
}

impl Autoload {
    /// Every path the autoload section refers to.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        let mapped = self
            .psr4
            .0
            .iter()
            .chain(self.psr0.0.iter())
            .flat_map(|(_, paths)| paths.iter());

        mapped
            .chain(self.classmap.iter().map(String::as_str))
            .chain(self.files.iter().map(String::as_str))
    }
}

/// A JSON string or an array of strings, as Composer allows for autoload
/// paths.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        let (one, many) = match self {
            StringOrList::One(path) => (Some(path.as_str()), &[][..]),
            StringOrList::Many(paths) => (None, paths.as_slice()),
        };

        one.into_iter().chain(many.iter().map(String::as_str))
    }
}

/// A JSON object whose key order is kept. Order matters for
/// `namespace_replacement_patterns` (priority) and for Composer's `psr-4`
/// (Strauss infers the prefix from the first key).
#[derive(Debug, Clone)]
pub struct Ordered<V>(pub Vec<(String, V)>);

impl<V> Ordered<V> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<V: Serialize> Serialize for Ordered<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(key, value)| (key, value)))
    }
}

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for OrderedVisitor<V> {
            type Value = Ordered<V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();

                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }

                Ok(Ordered(entries))
            }

            /// PHP writes an empty map as `[]`. Composer's own files do this.
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::invalid_type(
                        serde::de::Unexpected::Seq,
                        &"an object",
                    ));
                }

                Ok(Ordered(Vec::new()))
            }
        }

        deserializer.deserialize_any(OrderedVisitor(PhantomData))
    }
}

/// The parts of `composer.json` used to infer settings.
#[derive(Debug, Default, Deserialize)]
pub(super) struct ComposerJson {
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) autoload: Autoload,
    #[serde(default)]
    pub(super) require: Ordered<Value>,
    #[serde(default, rename = "require-dev")]
    pub(super) require_dev: Ordered<Value>,
    #[serde(default)]
    pub(super) config: ComposerConfig,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ComposerConfig {
    #[serde(rename = "vendor-dir")]
    pub(super) vendor_dir: Option<String>,
}
