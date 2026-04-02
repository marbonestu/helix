@file-tree @navigation
Feature: Keyboard navigation within the file tree

  Alex the developer can move through the file tree using Vim-style motion keys,
  expand and collapse directories, and open files — all without leaving the keyboard.

  Background:
    Given the file tree sidebar is visible and focused
    And the project contains the structure:
      """
      project/
        src/
          main.rs
          lib.rs
        tests/
          integration.rs
        Cargo.toml
        README.md
      """

  Rule: j and k move the selection one row at a time

    Example: j moves the selection down
      Given the root row is selected
      When Alex presses j
      Then the selection moves to src/

    Example: k moves the selection up
      Given src/ is selected
      When Alex presses k
      Then the selection moves to the root row

    Example: j at the last visible row does nothing
      Given README.md is the last visible row and is selected
      When Alex presses j
      Then README.md remains selected

    Example: k at the first row does nothing
      Given the root row is selected
      When Alex presses k
      Then the root row remains selected

  Rule: Arrow keys mirror j/k behavior

    Example: Down arrow moves the selection down
      Given the root row is selected
      When Alex presses the down arrow key
      Then the selection moves to src/

    Example: Up arrow moves the selection up
      Given src/ is selected
      When Alex presses the up arrow key
      Then the selection moves to the root row

  Rule: g and G jump to the list boundaries

    Example: G jumps to the last visible row
      Given the root row is selected
      When Alex presses G
      Then the last visible row is selected

    Example: g jumps to the first row
      Given README.md is selected
      When Alex presses g
      Then the root row is selected
      And the viewport scrolls to the top

  Rule: ctrl-u and ctrl-d scroll by half a viewport

    Example: ctrl-d moves selection down by half a page
      Given the root row is selected
      When Alex presses ctrl-d
      Then the selection advances by half the visible height
      And the selected row is still within the visible area

    Example: ctrl-u moves selection up by half a page
      Given a row in the middle of the tree is selected
      When Alex presses ctrl-u
      Then the selection moves back by half the visible height
      And the selected row is still within the visible area

  Rule: ctrl-b and ctrl-f scroll by a full viewport

    Example: ctrl-f moves selection down by a full page
      Given the root row is selected
      When Alex presses ctrl-f
      Then the selection advances by the full visible height

    Example: ctrl-b moves selection up by a full page
      Given a row near the bottom is selected
      When Alex presses ctrl-b
      Then the selection moves back by the full visible height

  Rule: ctrl-y and ctrl-e scroll the viewport without moving the selection

    Example: ctrl-e scrolls the viewport down while keeping the selection
      Given a row in the middle of the tree is selected
      When Alex presses ctrl-e
      Then the viewport scrolls down by one row
      And the same row remains selected

    Example: ctrl-y scrolls the viewport up while keeping the selection
      Given the viewport is scrolled down
      When Alex presses ctrl-y
      Then the viewport scrolls up by one row
      And the same row remains selected

  Rule: l and Enter expand a directory or open a file

    Example: l on a collapsed directory expands it
      Given src/ is selected and collapsed
      When Alex presses l
      Then src/ becomes expanded
      And main.rs and lib.rs appear beneath it

    Example: l on an expanded directory collapses it
      Given src/ is selected and expanded
      When Alex presses l
      Then src/ becomes collapsed
      And main.rs and lib.rs are hidden

    Example: Enter on a file opens it in the editor
      Given main.rs is selected
      When Alex presses Enter
      Then main.rs opens in the editor view
      And keyboard focus moves to the editor

    Example: l on a file opens it in the editor
      Given main.rs is selected
      When Alex presses l
      Then main.rs opens in the editor view
      And keyboard focus moves to the editor

  Rule: h collapses an open directory or jumps to its parent

    Example: h on an expanded directory collapses it
      Given src/ is selected and expanded
      When Alex presses h
      Then src/ becomes collapsed

    Example: h on a file moves selection to its parent directory
      Given main.rs is selected
      When Alex presses h
      Then src/ becomes selected

    Example: h on a collapsed directory moves selection to its parent
      Given src/ is selected and collapsed
      When Alex presses h
      Then the root row becomes selected

  Rule: R refreshes the tree from the filesystem

    Example: R reloads directory contents
      Given the file tree is showing an outdated listing
      When Alex presses R
      Then the tree re-scans the filesystem
      And any added or removed files are reflected in the listing
