@navigation:flash-search
Feature: Flash-powered incremental search

  Pressing "/" opens a flash search prompt rather than a plain regex prompt.
  All visible matches are highlighted as Alex types, and after jumping to a
  match the pattern is saved so that "n" and "N" continue navigating through
  the document without re-entering the search.

  Background:
    Given Alex has a file open in the editor

  Rule: "/" opens the flash search prompt

    Example: Flash prompt appears and shows a "search:" prefix in the status line
      Given Alex is in normal mode
      When Alex presses "/"
      Then the flash search prompt opens
      And the status line shows "search:"

  Rule: All visible matches are highlighted as Alex types

    Example: Every visible occurrence is highlighted when a pattern is entered
      Given the visible buffer contains three occurrences of "use"
      When Alex presses "/" and types "use"
      Then all three occurrences are selected and visually highlighted
      And jump labels are shown after each highlighted match

  Rule: Typing a label jumps to that match and saves the pattern for n/N

    Example: Jumping to a labeled match stores the query in the search register
      Given the visible buffer has two occurrences of "fn" with labels "a" and "b"
      When Alex presses "/", types "fn", then types "b"
      Then the cursor moves to the occurrence labeled "b"
      And the search pattern "fn" is saved to the "/" register

    Example: n moves forward to the next occurrence after a flash jump
      Given Alex has just jumped to an occurrence of "fn" using flash search
      When Alex presses "n"
      Then the cursor moves to the next occurrence of "fn" in the document

    Example: N moves backward to the previous occurrence after a flash jump
      Given Alex has just jumped to an occurrence of "fn" using flash search
      When Alex presses "N"
      Then the cursor moves to the previous occurrence of "fn" in the document

  Rule: Escape cancels the flash search and restores the original cursor position

    Example: Cancelling search mid-pattern returns the cursor to where it was
      Given Alex has typed "us" into the flash search prompt
      When Alex presses Escape
      Then the flash search prompt closes
      And the cursor returns to the position it was before Alex pressed "/"
      And all match highlights are cleared

  Rule: A single visible match triggers an automatic jump without label selection

    Example: Unique match jumps immediately and saves the pattern for n/N
      Given the visible buffer contains exactly one occurrence of "render_document"
      When Alex presses "/" and types "render_doc"
      Then the cursor jumps directly to "render_document"
      And the pattern is saved so that "n" will find the next occurrence in the file
