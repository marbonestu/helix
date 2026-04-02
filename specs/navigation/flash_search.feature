@navigation:flash-search
Feature: Flash-powered incremental search

  Pressing "/" opens a forward flash search prompt that only shows matches at or
  after the cursor. Pressing "?" opens a backward flash search prompt that only
  shows matches before the cursor, with the closest match labelled first.
  After jumping, the pattern is saved so that "n" and "N" continue navigating
  through the document.

  Rule: "/" opens the forward flash search prompt

    Example: Flash prompt appears and shows "/" in the status line
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "/"
      Then the status line shows "/"

  Rule: All visible matches at or after the cursor are highlighted as Alex types

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

  Rule: "?" opens the backward flash search prompt

    Example: "?" prompt appears and shows "?" in the status line
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "?"
      Then the status line shows "?"

  Rule: "?" only shows matches before the cursor position

    Example: "?" from the start of the buffer finds no matches
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "?" and types "fn"
      Then the status line shows "No matches"

    Example: "?" after moving the cursor forward finds matches above it
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "w"
      And Alex presses "?" and types "fn"
      Then the cursor is at position 0
      And the search register contains "fn"

    Example: Escape from "?" restores the cursor to where it was before pressing "?"
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "w"
      And Alex presses "?", types "fn", then presses Escape
      Then the cursor is at position 3
