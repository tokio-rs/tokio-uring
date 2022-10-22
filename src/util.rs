use std::marker::PhantomData;

/// Utility ZST for ensuring that opcodes are `!Send` and `!Sync`.
pub(crate) type PhantomUnsendUnsync = PhantomData<*mut ()>;
