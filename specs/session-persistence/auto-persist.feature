@session-persistence @auto-persist
Feature: Automatic session persistence on exit and startup

  Alex the developer wants Helix to silently save and restore the workspace
  without any manual commands, so that reopening the editor feels seamless.
  This behaviour is opt-in and controlled by the editor configuration.

  Rule: Auto-save on exit requires explicit opt-in

    Example: Helix saves the session on quit when persistence is enabled
      Given Alex has configured "persist = true" in "[editor.session]"
      And Alex has "/src/lib.rs" open with the cursor at line 10
      When Alex quits Helix with ":q"
      Then a session file is written for the current workspace
      And the session contains "/src/lib.rs" at line 10

    Example: Helix does not save a session on quit when persistence is disabled
      Given Alex has not enabled session persistence (default configuration)
      And Alex has "/src/lib.rs" open
      When Alex quits Helix with ":q"
      Then no session file is written for the current workspace

  Rule: Auto-restore on startup requires persistence to be enabled and no CLI files

    Example: Helix restores the previous session when opened with no files
      Given Alex has configured "persist = true" in "[editor.session]"
      And a session exists with "/src/main.rs" at line 20 column 3
      When Alex opens Helix without passing any files on the command line
      Then "/src/main.rs" is automatically opened at line 20 column 3

    Example: Helix does not restore a session when files are passed on the command line
      Given Alex has configured "persist = true" in "[editor.session]"
      And a session exists with "/src/main.rs" at line 20
      When Alex opens Helix with "hx /src/other.rs" on the command line
      Then "/src/other.rs" is opened instead of restoring the session

    Example: Helix opens a blank editor when persistence is disabled and no files are given
      Given Alex has not enabled session persistence (default configuration)
      When Alex opens Helix without passing any files on the command line
      Then a fresh scratch buffer is presented
