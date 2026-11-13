//! Interned face and edge names.

use std::collections::BTreeMap;
use std::fmt;

use crate::EdgeKey;

/// A name's id in a solid's [`Names`].
pub type Tag = u32;

/// A triangle's face names: sorted tags, shared between the triangles
/// that have the same ones (a part of a million triangles under one
/// name holds that name's list once). Read as a `Vec<Tag>`; change
/// with `Arc::make_mut`, or put another in its place.
pub type Tags = std::sync::Arc<Vec<Tag>>;

/// The strings behind a solid's tags, each stored once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    list: Vec<String>,
    index: BTreeMap<String, Tag>,
}

impl Names {
    /// The tag of `name`, added if new.
    pub fn intern(&mut self, name: &str) -> Tag {
        if let Some(&t) = self.index.get(name) {
            return t;
        }
        let t = self.list.len() as Tag;
        self.list.push(name.to_string());
        self.index.insert(name.to_string(), t);
        t
    }

    /// The tag of `name`, if it exists.
    pub fn get(&self, name: &str) -> Option<Tag> {
        self.index.get(name).copied()
    }

    /// The string of a tag.
    pub fn name(&self, tag: Tag) -> &str {
        &self.list[tag as usize]
    }

    /// Every name, sorted.
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(String::as_str)
    }

    /// Renumbers tags in name order, keeping only the used ones, so two
    /// solids with the same names have the same tags.
    pub(crate) fn compact(
        &mut self,
        face_tags: &mut [Tags],
        edge_tags: &mut BTreeMap<EdgeKey, Vec<Tag>>,
    ) {
        let mut used: Vec<bool> = vec![false; self.list.len()];
        for t in face_tags.iter().flat_map(|t| t.iter()) {
            used[*t as usize] = true;
        }
        for t in edge_tags.values().flatten() {
            used[*t as usize] = true;
        }
        let mut fresh = Names::default();
        let mut remap = vec![0; self.list.len()];
        for (name, &old) in &self.index {
            if used[old as usize] {
                remap[old as usize] = fresh.intern(name);
            }
        }
        // Each list once: triangles that shared one share its
        // renumbered one, and equal lists become one.
        let mut by_list: BTreeMap<usize, (Tags, Tags)> = BTreeMap::new();
        let mut by_tags: BTreeMap<Vec<Tag>, Tags> = BTreeMap::new();
        for tags in face_tags.iter_mut() {
            let key = std::sync::Arc::as_ptr(tags) as usize;
            if let Some((_, done)) = by_list.get(&key) {
                *tags = done.clone();
                continue;
            }
            let mut renumbered: Vec<Tag> = tags.iter().map(|t| remap[*t as usize]).collect();
            renumbered.sort_unstable();
            let shared = match by_tags.get(&renumbered) {
                Some(s) => s.clone(),
                None => {
                    let s: Tags = std::sync::Arc::new(renumbered.clone());
                    by_tags.insert(renumbered, s.clone());
                    s
                }
            };
            // The old list is held until the end, so its address stays
            // its own.
            by_list.insert(key, (tags.clone(), shared.clone()));
            *tags = shared;
        }
        for tags in edge_tags.values_mut() {
            for t in tags.iter_mut() {
                *t = remap[*t as usize];
            }
            tags.sort_unstable();
        }
        *self = fresh;
    }
}

/// A lookup named something nothing carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookupError {
    /// The names asked for that nothing carries.
    pub missing: Vec<String>,
    /// The names that do exist, sorted.
    pub available: Vec<String>,
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no element is named {}; names: {}",
            self.missing.join(", "),
            self.available.join(", ")
        )
    }
}

impl std::error::Error for LookupError {}
