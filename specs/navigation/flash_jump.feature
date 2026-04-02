@navigation:flash-jump
Feature: Flash jump navigation

  Alex the developer can jump to any visible text on screen by typing a
  short search prefix. Labels appear on all matches and update as Alex
  narrows the pattern, keeping both hands on the keyboard at all times.

  Rule: Labels appear on all visible matches after the first character is typed

    Example: Typing a prefix keeps the flash prompt open when multiple matches exist
      Given the buffer contains "hello\nhelix\nhelp\n"
      When Alex presses "gS" and types "he"
      Then the cursor has not moved from the start of the buffer

    Example: No visible matches closes the prompt immediately
      Given the buffer contains "apple\nbanana\n"
      When Alex presses "gS" and types "z"
      Then the cursor is at the start of the buffer

  Rule: Typing a label character jumps to the corresponding match

    Example: Typing the second label moves the cursor to the second match
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "gS", types "fn", then types "b"
      Then the cursor is at position 7

    Example: The original position is saved to the jumplist after a jump
      Given the buffer contains "fn foo\nfn bar\n"
      When Alex presses "gS", types "fn", then types "b"
      Then the jumplist has grown by one entry

  Rule: A single remaining match triggers an automatic jump

    Example: Unique match auto-jumps without a label keystroke
      Given the buffer contains "hello\nworld\n"
      When Alex presses "gS" and types "hel"
      Then the cursor is at position 0

  Rule: Labels avoid characters that would continue the search pattern

    Example: Typing a continuation character extends the search to the unique match
      Given the buffer contains "hello\nhelp\nhelium\n"
      When Alex presses "gS", types "hel", then types "l"
      Then the cursor is at position 0

  Rule: Backspace removes the last typed character and widens the match set

    Example: Backspace after a multi-match query keeps the prompt open
      Given the buffer contains "hello\nhelix\nhelium\n"
      When Alex presses "gS", types "heli", then presses Backspace
      Then the cursor has not moved from the start of the buffer

  Rule: Escape cancels the jump and restores the original cursor position

    Example: Escape during label selection returns the cursor to its original position
      Given the buffer contains "hello\nhelix\nhelp\n"
      When Alex presses "gS", types "he", then presses Escape
      Then the cursor is at the start of the buffer

  Rule: Flash jump in select mode extends the selection to the target

    Example: In select mode the selection anchor stays and the head moves to the target
      Given the buffer contains "start middle end\n"
      When Alex enters select mode, presses "S", and types "end"
      Then the cursor is at position 14
      And the selection anchor is at position 0
