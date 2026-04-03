@navigation:ts-flash-jump
Feature: Treesitter flash jump navigation

  Alex the developer can jump to any visible treesitter object of a specific
  type by pressing the corresponding bracket navigation key. Labels appear on
  all visible objects of that type within the viewport, filtered by direction,
  so Alex can jump directly to any target without repeatedly pressing ]f.

  Rule: ]f labels all visible functions at or after the cursor in the viewport

    Example: Two visible functions both receive labels; typing the second label jumps there
      Given the Rust buffer contains "fn foo() {}\nfn bar() {}\n"
      When Alex presses "]f" and types "b"
      Then the cursor is at position 12

    Example: Single visible function auto-jumps without requiring a label keystroke
      Given the Rust buffer contains "fn only_one() {}\n"
      When Alex presses "]f"
      Then the cursor is at position 0

    Example: No visible functions shows "No matches" and leaves the cursor in place
      Given the Rust buffer contains "struct Foo {}\n"
      When Alex presses "]f"
      Then the cursor is at the start of the buffer
      And the status line shows "No matches"

  Rule: Only objects of the requested type receive labels

    Example: ]f labels only functions; struct definitions are not labelled
      Given the Rust buffer contains "struct Foo {}\nfn bar() {}\n"
      When Alex presses "]f" and types "a"
      Then the cursor is at position 14

    Example: ]t labels only type definitions; functions are not labelled
      Given the Rust buffer contains "struct Foo {}\nfn bar() {}\n"
      When Alex presses "]t" and types "a"
      Then the cursor is at position 0

    Example: ]a labels only parameters; function keyword is not labelled
      Given the Rust buffer contains "fn foo(x: i32, y: i32) {}\n"
      When Alex presses "]a" and types "b"
      Then the cursor is at position 15

  Rule: Escape cancels the ts-flash jump and restores the original cursor position

    Example: Escape during label selection returns the cursor to its original position
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
