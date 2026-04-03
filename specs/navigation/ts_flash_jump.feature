@navigation:ts-flash-jump
Feature: Treesitter flash jump navigation

  Alex the developer can jump to any visible treesitter object of a specific
  type by pressing the corresponding bracket navigation key. On activation the
  cursor immediately moves to the first (nearest) object, labels appear on all
  visible objects of that type, and Alex can press a label to jump to any
  other target. The full span of the object is selected, matching the behaviour
  of the existing goto_next/prev_* commands.

  Rule: ]f immediately jumps to the first visible function and labels the rest

    Example: ]f moves the cursor to the first function right away
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "]f"
      Then the cursor is at position 10

    Example: Typing a label after ]f jumps to the labelled function
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "]f" and types "b"
      Then the cursor is at position 22

    Example: Single visible function auto-jumps without requiring a label keystroke
      Given the Rust buffer contains "fn only_one() {}\n"
      When Alex presses "]f"
      Then the cursor is at position 15

    Example: No visible functions shows "No matches" and leaves the cursor in place
      Given the Rust buffer contains "struct Foo {}\n"
      When Alex presses "]f"
      Then the cursor is at the start of the buffer
      And the status line shows "No matches"

  Rule: Only objects of the requested type receive labels

    Example: ]f labels only functions; struct definitions are not labelled
      Given the Rust buffer contains "struct Foo {}\nfn bar() {}\n"
      When Alex presses "]f" and types "a"
      Then the cursor is at position 24

    Example: ]t labels only type definitions; functions are not labelled
      Given the Rust buffer contains "struct Foo {}\nfn bar() {}\n"
      When Alex presses "]t" and types "a"
      Then the cursor is at position 12

    Example: ]a labels only parameters; function keyword is not labelled
      Given the Rust buffer contains "fn foo(x: i32, y: i32) {}\n"
      When Alex presses "]a" and types "b"
      Then the cursor is at position 20

  Rule: Escape cancels and restores the cursor to where it was before ]f

    Example: Escape returns the cursor to its original position
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "]f", types "b", then presses Escape
      Then the cursor is at the start of the buffer

  Rule: [f labels visible functions strictly before the cursor

    Example: [f from the second function labels the first function above
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "j"
      And Alex presses "[f" and types "a"
      Then the cursor is at position 0

    Example: [f from the start of the buffer shows "No matches"
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "[f"
      Then the cursor is at the start of the buffer
      And the status line shows "No matches"

  Rule: A ts-flash jump is recorded in the jumplist

    Example: Jumping via a ts-flash label adds an entry to the jumplist enabling jump-back
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "]f" and types "b"
      Then the jumplist has grown by one entry
