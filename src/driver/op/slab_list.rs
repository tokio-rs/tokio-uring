//! An indexed linked list, with entries held in slab storage.
//! The slab may hold multiple independent lists concurrently.
//!
//! Each list is uniquely identified by a SlabListIndices,
//! which holds the index of the first element of the list.
//! It also holds the index of the last element, to support
//! push operations without list traversal.
use slab::Slab;
use std::ops::{Deref, DerefMut};

/// A linked list backed by slab storage
pub(crate) struct SlabList<'a, T> {
    index: SlabListIndices,
    slab: &'a mut Slab<SlabListEntry<T>>,
}

// Indices to the head and tail of a single list held within a SlabList
pub(crate) struct SlabListIndices {
    start: usize,
    end: usize,
}

/// Multi cycle operations may return an unbounded number of CQE's
/// for a single cycle SQE.
///
/// These are held in an indexed linked list
pub(crate) struct SlabListEntry<T> {
    entry: T,
    next: usize,
}

impl<T> Deref for SlabListEntry<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl<T> DerefMut for SlabListEntry<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

impl SlabListIndices {
    pub(crate) fn new() -> Self {
        let start = usize::MAX;
        SlabListIndices { start, end: start }
    }

    pub(crate) fn into_list<T>(self, slab: &mut Slab<SlabListEntry<T>>) -> SlabList<'_, T> {
        SlabList::from_indices(self, slab)
    }
}

impl<'a, T> SlabList<'a, T> {
    pub(crate) fn from_indices(
        index: SlabListIndices,
        slab: &'a mut Slab<SlabListEntry<T>>,
    ) -> Self {
        SlabList { slab, index }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.index.start == usize::MAX
    }

    /// Peek at the end of the list (most recently pushed)
    /// This leaves the list unchanged
    pub(crate) fn peek_end(&mut self) -> Option<&T> {
        if self.index.end == usize::MAX {
            None
        } else {
            Some(&self.slab[self.index.end].entry)
        }
    }

    /// Pop from front of list
    #[allow(dead_code)]
    pub(crate) fn pop(&mut self) -> Option<T> {
        self.slab
            .try_remove(self.index.start)
            .map(|SlabListEntry { next, entry, .. }| {
                if next == usize::MAX {
                    self.index.end = usize::MAX;
                }
                self.index.start = next;
                entry
            })
    }

    /// Push to the end of the list
    pub(crate) fn push(&mut self, entry: T) {
        let prev = self.index.end;
        let entry = SlabListEntry {
            entry,
            next: usize::MAX,
        };
        self.index.end = self.slab.insert(entry);
        if prev != usize::MAX {
            self.slab[prev].next = self.index.end;
        } else {
            self.index.start = self.index.end;
        }
    }

    /// Consume the list, without dropping entries, returning just the start and end indices
    pub(crate) fn into_indices(mut self) -> SlabListIndices {
        std::mem::replace(&mut self.index, SlabListIndices::new())
    }
}

impl<'a, T> Drop for SlabList<'a, T> {
    fn drop(&mut self) {
        while !self.is_empty() {
            let removed = self.slab.remove(self.index.start);
            self.index.start = removed.next;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn push_pop() {
        let mut slab = Slab::with_capacity(8);
        let mut list = SlabListIndices::new().into_list(&mut slab);
        assert!(list.is_empty());
        assert_eq!(list.pop(), None);
        for i in 0..5 {
            list.push(i);
            assert_eq!(list.peek_end(), Some(&i));
            assert!(!list.is_empty());
            assert!(!list.slab.is_empty());
        }
        for i in 0..5 {
            assert_eq!(list.pop(), Some(i))
        }
        assert!(list.is_empty());
        assert!(list.slab.is_empty());
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn entries_freed_on_drop() {
        let mut slab = Slab::with_capacity(8);
        {
            let mut list = SlabListIndices::new().into_list(&mut slab);
            list.push(42);
            assert!(!list.is_empty());
        }
        assert!(slab.is_empty());
    }

    #[test]
    fn entries_kept_on_converion_to_index() {
        let mut slab = Slab::with_capacity(8);
        {
            let mut list = SlabListIndices::new().into_list(&mut slab);
            list.push(42);
            assert!(!list.is_empty());
            // This forgets the entries
            let _ = list.into_indices();
        }
        assert!(!slab.is_empty());
    }
}
