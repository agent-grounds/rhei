### Task 1: Claim issue 87
**State:** claim

Assign issue 87 to this machine before anything is spent on it.

### Task 2: Work issue 87
**State:** work
**Prior:** Task 1

Speak for the fix and the gate below: the ticket's result is what a reader of
issue 87 gets.

#### Task 2.1: Fix the parser
**State:** work
**Prior:** Task 1

Make the parser reject the literal with a diagnostic instead of panicking.

#### Task 2.2: Run the gate
**State:** work
**Prior:** Task 2.1

Run the test suite against the fixed parser.
