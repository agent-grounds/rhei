### Task 1: Report how storage survives a crash
**State:** work

Say how the storage module survives a crash at any point, from the three
answers below.

#### Task 1.1: When is the WAL flushed?
**State:** work

Read `wal.rs` and say when the log reaches disk.

#### Task 1.2: When is a page fsynced?
**State:** work

Read `pager.rs` and say when a page reaches disk.

#### Task 1.3: What does recovery replay?
**State:** work

Read `recovery.rs` and say what is replayed after a crash.
