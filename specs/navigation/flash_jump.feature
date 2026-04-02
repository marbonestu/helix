@navigation:flash-jump
Feature: Flash jump navigation

  Alex the developer can jump to any visible text on screen by typing a
  short search prefix. Labels appear on all matches and update as Alex
  narrows the pattern, keeping both hands on the keyboard at all times.

  Background:
    Given Alex has a file open in the editor

  Rule: Labels appear on all visible matches after the first character is typed

    Example: Single character with multiple matches shows labeled targets
      Given the visible buffer contains the words "hello", "helix", and "help"
      When Alex presses "gS" and types "he"
      Then each word starting with "he" shows a jump label after the matched text
      And the matched text is highlighted across all visible matches

    Example: No visible matches closes the prompt immediately
      Given the visible buffer contains no word starting with "z"
      When Alex presses "gS" and types "z"
      Then the flash prompt closes
      And the cursor returns to its original position

  Rule: Typing a label character jumps to the corresponding match

    Example: Alex selects a label and lands on that match
      Given the visible buffer has two matches for "fn" with labels "a" and "b"
      When Alex presses "gS", types "fn", then types "b"
      Then the cursor moves to the start of the match labeled "b"
      And the original position is saved to the jumplist

    Example: Typing a non-label character extends the search instead of jumping
      Given the visible buffer has matches for "he" with label "a" on "hello"
      When Alex presses "gS", types "he", then types "l"
      Then the search narrows to matches for "hel"
      And labels are reassigned to the remaining matches

  Rule: A single remaining match triggers an automatic jump

    Example: Narrowing to one match jumps without requiring a label keystroke
      Given the visible buffer contains "hello" but no other word starting with "hel"
      When Alex presses "gS" and types "hel"
      Then the cursor jumps directly to "hello"
      And no label selection step is required

  Rule: Labels avoid characters that would continue the search pattern

    Example: Label pool excludes characters present after each match
      Given the visible buffer contains "hello" and "help"
      When Alex presses "gS" and types "hel"
      Then labels "l" and "p" are not assigned to any match
      And the remaining alphabet letters are used as labels instead

  Rule: Backspace narrows the query back one character

    Example: Backspace widens the match set by removing the last character
      Given Alex has typed "hel" and sees one match
      When Alex presses Backspace
      Then the search reverts to "he" and all matches for "he" reappear with new labels

  Rule: Escape cancels the jump and restores the original cursor position

    Example: Escape during label selection returns to the original position
      Given Alex has typed "he" and sees several labeled matches
      When Alex presses Escape
      Then the flash prompt closes
      And the cursor is at the position it was before Alex pressed "gS"
      And all jump label overlays are removed

  Rule: Flash jump in select mode extends the selection to the target

    Example: Extending the selection from the current position to a jump target
      Given Alex is in select mode with the cursor on the word "start"
      When Alex presses "S" and types a search pattern that resolves to "end"
      Then the selection stretches from "start" to "end"
