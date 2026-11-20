//! Outcomes for synchronous Metamod hooks.

/// A pre-hook decision. The value is used only for replacement outcomes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HookResult<T> {
    #[default]
    Ignored,
    Handled,
    Override(T),
    Supercede(T),
}

/// A post-hook decision. The original call has already run or been skipped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PostHookResult<T> {
    #[default]
    Ignored,
    Handled,
    Override(T),
}
