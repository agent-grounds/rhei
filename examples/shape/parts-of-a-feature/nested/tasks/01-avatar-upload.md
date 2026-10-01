### Task 1: Avatar upload
**State:** work

The avatar feature. The three parts below are its work.

#### Task 1.1: Add the avatar column
**State:** work

Add a nullable `avatar_url` column to `users`, with its migration.

#### Task 1.2: Add the upload endpoint
**State:** work
**Prior:** Task 1.1

Add `PUT /users/{id}/avatar`: store the image and write its URL to the column.

#### Task 1.3: Show the avatar on the profile page
**State:** work
**Prior:** Task 1.2

Render the avatar on the profile page, and the user's initials when there is
none.
