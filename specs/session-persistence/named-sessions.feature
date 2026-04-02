@session-persistence @named-sessions
Feature: Named sessions

  Alex the developer switches between distinct contexts — a feature branch,
  a bug fix, a code review — each with its own set of open files and splits.
  Named sessions let Alex bookmark each context and jump back to it by name.

  Background:
    Given Alex has Helix open in a project directory

  Rule: A named session is saved and restored independently from the default session

    Example: Saving a named session does not overwrite the default session
      Given Alex has a saved default session with "/src/main.rs" open
      And Alex now has "/src/feature.rs" open
      When Alex runs ":session-save feature-x"
      Then the named session "feature-x" records "/src/feature.rs"
      And the default session still records "/src/main.rs"

    Example: Restoring a named session opens its recorded files
      Given Alex has a named session "feature-x" with "/src/feature.rs" at line 5
      When Alex runs ":session-restore feature-x"
      Then "/src/feature.rs" is open at line 5
      And Alex sees the status message "Session restored"

  Rule: Named sessions can be listed

    Example: Session list shows all saved sessions with their working directory and file count
      Given Alex has a default session with 3 open files
      And a named session "bugfix" with 2 open files
      And a named session "review" with 1 open file
      When Alex runs ":session-list"
      Then Alex sees three entries: "session", "bugfix", and "review"
      And each entry shows its file count and working directory

    Example: Session list is empty when no sessions have been saved
      Given no sessions have been saved for the current workspace
      When Alex runs ":session-list"
      Then Alex sees an empty list or a message indicating no sessions exist

  Rule: A named session can be deleted independently

    Example: Deleting a named session does not affect other sessions
      Given Alex has a named session "feature-x" and a named session "bugfix"
      When Alex runs ":session-delete feature-x"
      Then the "feature-x" session no longer exists
      And the "bugfix" session is unaffected

  Rule: Restoring a non-existent named session reports a clear error

    Example: Attempting to restore an unknown session name shows an informative message
      Given no session named "nonexistent" exists
      When Alex runs ":session-restore nonexistent"
      Then Alex sees an error message indicating the session was not found
      And the editor state is unchanged
