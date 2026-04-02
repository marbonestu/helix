@session-persistence
Feature: Basic session save and restore

  Alex the developer works on a project across multiple editing sessions.
  Being able to explicitly save and restore the editor state lets Alex pick
  up exactly where they left off without manually reopening every file.

  Background:
    Given Alex has Helix open in a project directory

  Rule: A saved session captures the open file and cursor position

    Example: Saving a session records the current file and cursor location
      Given Alex has "/src/main.rs" open with the cursor at line 42 column 7
      When Alex runs ":session-save"
      Then a session file is written for the current workspace
      And the session records "/src/main.rs" as the open file
      And the session records the cursor at line 42 column 7

    Example: Restoring a session reopens the file at the saved cursor position
      Given Alex has a saved session with "/src/main.rs" at line 42 column 7
      And Helix is opened in the same workspace without specifying any files
      When Alex runs ":session-restore"
      Then "/src/main.rs" is open in the editor
      And the cursor is at line 42 column 7
      And Alex sees the status message "Session restored"

    Example: Saving a session with a scratch buffer preserves it as a new scratch buffer
      Given Alex has an unsaved scratch buffer open with no file path
      When Alex runs ":session-save"
      Then the session records a scratch buffer entry
      When Alex restores the session
      Then a new scratch buffer is opened in place of the saved one

  Rule: A session can be explicitly deleted

    Example: Deleting a session removes its file from disk
      Given Alex has a saved session for the current workspace
      When Alex runs ":session-delete"
      Then the session file no longer exists on disk
      And Alex sees the status message "Session deleted"

    Example: Deleting a session when none exists completes without error
      Given no session file exists for the current workspace
      When Alex runs ":session-delete"
      Then no error is reported

  Rule: Restoring when no session exists reports a clear error

    Example: Attempting to restore with no saved session shows an informative message
      Given no session file exists for the current workspace
      When Alex runs ":session-restore"
      Then Alex sees an error message indicating no session was found
      And the editor state is unchanged
