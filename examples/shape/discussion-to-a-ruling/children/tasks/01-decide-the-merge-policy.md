### Task 1: Rule on the merge policy
**State:** work
**Provides:** ruling

Rule on how the merge queue merges, from the round the judge closed, and
export the ruling as `ruling`.

#### Task 1.1: Round 1: claude's position
**State:** work

Argue the merge policy from readable history.

#### Task 1.2: Round 1: codex's position
**State:** work

Argue the merge policy from bisect.

#### Task 1.3: Round 1: judge the round
**State:** work
**Prior:** Task 1.1, Task 1.2

Digest the round, and rule if the positions converged.

#### Task 1.4: Round 2: claude's position
**State:** work
**Prior:** Task 1.3

Answer codex's round 1 position.

#### Task 1.5: Round 2: codex's position
**State:** work
**Prior:** Task 1.3

Answer claude's round 1 position.

#### Task 1.6: Round 2: judge the round
**State:** work
**Prior:** Task 1.4, Task 1.5

Digest the round, and rule if the positions converged.

### Task 2: Apply the merge policy
**State:** work
**Prior:** Task 1
**Consumes:** 1:ruling

Configure the merge queue the way the ruling says.
