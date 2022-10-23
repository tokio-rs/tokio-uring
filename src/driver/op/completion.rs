use slab::Slab;
use std::ops::{Deref, DerefMut};

use crate::driver::op::CqeResult;

/// A linked list of CQE events
pub(crate) struct CompletionList<'a> {
    index: CompletionIndices,
    completions: &'a mut Slab<Completion>,
}

// Index to the first and last Completion of a single list held in the slab
pub(crate) struct CompletionIndices {
    start: usize,
    end: usize,
}

/// Multi cycle operations may return an unbounded number of CQE's
/// for a single cycle SQE.
///
/// These are held in an indexed linked list
pub(crate) struct Completion {
    cqe: CqeResult,
    next: usize,
    prev: usize,
}

impl Deref for Completion {
    type Target = CqeResult;

    fn deref(&self) -> &Self::Target {
        &self.cqe
    }
}

impl DerefMut for Completion {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cqe
    }
}

impl CompletionIndices {
    pub(crate) fn new() -> Self {
        let start = usize::MAX;
        CompletionIndices { start, end: start }
    }

    pub(crate) fn into_list(self, completions: &mut Slab<Completion>) -> CompletionList<'_> {
        CompletionList::from_indices(self, completions)
    }
}

impl<'a> CompletionList<'a> {
    pub(crate) fn from_indices(
        index: CompletionIndices,
        completions: &'a mut Slab<Completion>,
    ) -> Self {
        CompletionList { completions, index }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.index.start == usize::MAX
    }

    /// Peek at the end of the list (most recently pushed)
    /// This leaves the list unchanged
    pub(crate) fn peek_end(&mut self) -> Option<&CqeResult> {
        if self.index.end == usize::MAX {
            None
        } else {
            Some(&self.completions[self.index.end].cqe)
        }
    }

    /// Pop from front of list
    #[allow(dead_code)]
    pub(crate) fn pop(&mut self) -> Option<CqeResult> {
        self.completions
            .try_remove(self.index.start)
            .map(|Completion { next, cqe, .. }| {
                if next != usize::MAX {
                    self.completions[next].prev = usize::MAX;
                } else {
                    self.index.end = usize::MAX;
                }
                self.index.start = next;
                cqe
            })
    }

    /// Push to the end of the list
    pub(crate) fn push(&mut self, cqe: CqeResult) {
        let prev = self.index.end;
        let completion = Completion {
            cqe,
            next: usize::MAX,
            prev,
        };
        self.index.end = self.completions.insert(completion);
        self.completions[prev].next = self.index.end;
    }

    /// Consume the list, without dropping entries, returning just the start and end indices
    pub(crate) fn into_indices(mut self) -> CompletionIndices {
        std::mem::replace(&mut self.index, CompletionIndices::new())
    }
}

impl<'a> Drop for CompletionList<'a> {
    fn drop(&mut self) {
        while !self.is_empty() {
            let removed = self.completions.remove(self.index.start);
            self.index.start = removed.next;
        }
    }
}
