# Implementer Handoff

The plan-wide spec review failed on two bounded acceptance gaps.

## Acceptance criteria

1. Given a complete frontend catalog containing two or more equal canonical address names, all duplicate names are identified and the user receives one visible repair warning per stable conflict set. The warning does not block catalog synchronization and tells the user to rename the affected panes.
2. Given a Rust catalog record whose `terminal_id` is not a UUID, synchronization returns `INVALID_REQUEST` and leaves the existing directory unchanged.
3. Existing duplicate masking, runtime first-owner-wins behavior, persistence ordering, and catalog synchronization behavior remain covered and passing.
4. Focused frontend and Rust regression tests pass, followed by the full relevant frontend and Rust gates.
