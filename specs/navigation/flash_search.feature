@navigation:flash-search
Feature: Flash-powered incremental search

  Pressing "/" opens a flash search prompt rather than a plain regex prompt.
  All visible matches are highlighted as Alex types, and after jumping to a
  match the pattern is saved so that "n" and "N" continue navigating through
  the document without re-entering the search.

  Rule: "/" opens the flash search prompt

    Example: Flash prompt appears and shows a "search:" prefix in the status line
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "/"
      Then the status line shows "search:"

  Rule: All visible matches are highlighted as Alex types

    Example: Typing a label after a multi-match prefix jumps to the labeled target
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "/", types "fn", then types "a"
      Then the cursor is at position 0

  Rule: Typing a label jumps to that match and saves the pattern for n/N

    Example: Jumping to a labeled match stores the query in the search register
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "/", types "fn", then types "b"
      Then the cursor is at position 7
      And the search register contains "fn"

    Example: "n" moves forward to the next occurrence after a flash jump
      Given the buffer contains "fn foo\nfn bar\nfn baz\n"
      When Alex presses "/", types "fn", types "b", then presses "n"
      Then the cursor is at position 15

    Example: "N" moves backward to the previous occurrence after a flash jump
      Given the buffer contains "fn foo\nfn bar\nfn baz\n"
      When Alex presses "/", types "fn", types "c", then presses "N"
      Then the cursor is at position 8

  Rule: Escape cancels the flash search and restores the original cursor position

    Example: Escape mid-pattern returns the cursor to where it was before pressing "/"
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "/", types "fn", then presses Escape
      Then the cursor is at the start of the buffer

  Rule: A single visible match triggers an automatic jump without label selection

    Example: Unique match jumps immediately and saves the pattern for n/N
      Given the buffer contains "hello\nhem\n"
      When Alex presses "/" and types "hel"
      Then the cursor is at position 0
      And the search register contains "hel"
