@file-tree @sidebar
Feature: File tree sidebar visibility and focus

  Alex the developer can show or hide a docked file tree panel on the left side
  of the editor and move keyboard focus between the tree and the open buffers.

  Background:
    Given Alex has opened Helix in a project directory

  Rule: The sidebar can be toggled on and off

    Example: Opening the sidebar reveals the project tree
      Given the file tree sidebar is hidden
      When Alex presses space-e
      Then the file tree sidebar becomes visible
      And the root directory name is shown at the top of the panel
      And the root's immediate children are listed below it

    Example: Closing the sidebar frees up horizontal space
      Given the file tree sidebar is visible
      When Alex presses space-e
      Then the file tree sidebar is hidden
      And the editor views expand to fill the full terminal width

    Example: Reopening the sidebar restores the previous tree state
      Given Alex has expanded the src/ directory in the tree
      And Alex closes and reopens the sidebar
      When the sidebar reappears
      Then src/ is still shown as expanded

  Rule: Root children are visible immediately on first open

    Example: First open shows children without a second toggle
      Given the file tree sidebar has never been opened in this session
      When Alex presses space-e
      Then the root directory's children are visible immediately
      And Alex does not need to press any additional key

  Rule: Focus can move between the tree and the editor

    Example: Moving left from the leftmost editor split focuses the tree
      Given the file tree sidebar is visible
      And focus is on an editor split with no split to its left
      When Alex presses ctrl-w h
      Then keyboard focus moves into the file tree sidebar
      And the currently selected row is highlighted

    Example: Moving right from the tree returns focus to the editor
      Given keyboard focus is in the file tree sidebar
      When Alex presses ctrl-w l
      Then keyboard focus returns to the editor split
      And the file tree sidebar remains visible

    Example: Pressing q from the tree closes the sidebar
      Given keyboard focus is in the file tree sidebar
      When Alex presses q
      Then the file tree sidebar is hidden

    Example: Pressing escape from the tree unfocuses without closing
      Given keyboard focus is in the file tree sidebar
      When Alex presses escape
      Then keyboard focus returns to the editor split

  Rule: The current buffer's file can be revealed in the tree

    Example: Reveal opens the sidebar when it is hidden
      Given the file tree sidebar is hidden
      And Alex has opened src/main.rs in the editor
      When Alex runs the reveal-in-file-tree command
      Then the file tree sidebar becomes visible
      And keyboard focus moves into the file tree sidebar
      And src/main.rs is selected in the file tree

    Example: Reveal focuses the tree when it is already visible
      Given the file tree sidebar is visible
      And Alex has opened src/main.rs in the editor
      When Alex runs the reveal-in-file-tree command
      Then keyboard focus moves into the file tree sidebar
      And src/main.rs is selected in the file tree
