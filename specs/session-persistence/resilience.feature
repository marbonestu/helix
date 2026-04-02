@session-persistence @resilience
Feature: Session resilience and edge case handling

  Alex the developer works in a dynamic environment — files get deleted,
  branches get switched, content shrinks. Helix handles these situations
  gracefully so a stale session never causes a crash or silent data loss.

  Rule: Files that no longer exist on disk are skipped during restore

    Example: A missing file is skipped and remaining files are still restored
      Given Alex has a saved session with "/src/main.rs" and "/src/deleted.rs"
      And "/src/deleted.rs" has been removed from disk
      When Alex restores the session
      Then "/src/main.rs" is opened successfully
      And Alex sees a warning that "/src/deleted.rs" could not be found
      And the editor does not crash or show an error dialog

    Example: A session where all files are missing results in a clear message
      Given Alex has a saved session but all recorded files have been deleted
      When Alex restores the session
      Then Alex sees a message indicating no files from the session could be restored

  Rule: Cursor positions are clamped when a file has shrunk since the session was saved

    Example: A cursor beyond the end of a shorter file is moved to the last valid position
      Given Alex has a saved session with the cursor at line 200 of "/src/main.rs"
      And "/src/main.rs" now contains only 50 lines
      When Alex restores the session
      Then "/src/main.rs" is opened with the cursor at or before line 50
      And the editor does not crash

  Rule: Session files from older schema versions are handled gracefully

    Example: A session file written by an older version of Helix still loads
      Given a session file on disk that uses an earlier schema version
      When Alex restores the session
      Then the session loads without error
      And any fields absent in the older format use sensible defaults

    Example: A session file written by a newer version of Helix loads without crashing
      Given a session file on disk that includes fields from a future schema version
      When Alex restores the session
      Then the session loads and unknown fields are ignored
      And the known fields are restored correctly

  Rule: A corrupt or unreadable session file is handled without crashing

    Example: A session file containing invalid JSON reports an error and leaves the editor usable
      Given the session file for the current workspace contains malformed JSON
      When Alex runs ":session-restore"
      Then Alex sees an error message describing the problem
      And the editor remains open with an empty or unchanged state

  Rule: Saving a session is a safe operation even under adverse filesystem conditions

    Example: A session save to a non-writable location reports a clear error
      Given the session directory is not writable
      When Alex runs ":session-save"
      Then Alex sees an error message indicating the session could not be saved
      And no partial or corrupt session file is left on disk
