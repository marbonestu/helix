use std::num::NonZeroUsize;

use helix_view::input::KeyEvent;

/// Pending vim operator waiting for a motion or text-object.
#[derive(Debug, Clone)]
pub struct PendingOperator {
    pub op: Operator,
    /// Count before the operator (the `2` in `2dw`)
    pub pre_count: Option<NonZeroUsize>,
    /// Register prefix (the `a` in `"adw`)
    pub register: Option<char>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Yank,
    Change,
    Indent,
    Unindent,
    Uppercase,
    Lowercase,
    SwitchCase,
}

impl Operator {
    /// The key character that triggers this operator (used for doubled-operator detection).
    pub fn key_char(&self) -> char {
        match self {
            Operator::Delete => 'd',
            Operator::Yank => 'y',
            Operator::Change => 'c',
            Operator::Indent => '>',
            Operator::Unindent => '<',
            // These compound operators don't have a single-key doubled form
            Operator::Uppercase => '\0',
            Operator::Lowercase => '\0',
            Operator::SwitchCase => '\0',
        }
    }
}

/// Recorded action for dot-repeat.
#[derive(Debug, Clone)]
pub struct VimRepeatAction {
    pub register: Option<char>,
    pub total_count: usize,
    pub op: Operator,
    pub motion_keys: Vec<KeyEvent>,
}
