### Task 1: Report whether a crash can lose a committed write
**State:** work

Say whether the database can lose a committed write in a crash at any point,
from the three modules' answers below.

#### Task 1.1: Can a crash lose a committed write in the WAL?
**State:** work

Read `wal.rs` and answer for the write-ahead log.

#### Task 1.2: Can a crash lose a committed write in the pager?
**State:** work

Read `pager.rs` and answer for the pager.

#### Task 1.3: Can a crash lose a committed write in recovery?
**State:** work

Read `recovery.rs` and answer for recovery.
