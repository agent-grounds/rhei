"""Mock agent for the parts-of-a-feature shape pair.

Stands in for a real coding agent so both shapes run with no credentials. The
work is keyed by task title, not id, so the flat and the nested shape — the
same work authored two ways — write the same results, and the only difference
left between their runs is the shape.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import pathlib
import re
import sys

RESULTS = {
    'Add the avatar column':
        'Added the nullable `avatar_url` column to `users`, with migration 0042.',
    'Add the upload endpoint':
        'Added `PUT /users/{id}/avatar`; it stores the image and writes its URL to `avatar_url`.',
    'Show the avatar on the profile page':
        'The profile page shows the avatar, and the initials when there is none.',
    # All the parent of the nested shape can say: the children below are done.
    'Avatar upload':
        'The three parts below are done.',
}

EXPORTS = {}


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def prompt_arg():
    """The authoritative autonomous context delivered by `prompt_flag`."""
    args = sys.argv[1:]
    for index, arg in enumerate(args[:-1]):
        if arg == '--prompt':
            return args[index + 1]
    return ''


prompt = prompt_arg()
root_match = re.search(r'^- This rhei: `([^`]+)`', prompt, re.MULTILINE)
task_match = re.search(r'^#+ Task ([^:]+): (.+)$', prompt, re.MULTILINE)
if not root_match or not task_match:
    sys.exit('the agent prompt is missing its Rhei execution root or task')

root = pathlib.Path(root_match.group(1))
task, title = task_match.group(1), task_match.group(2).strip()
result_section = prompt.split('\n## Result\n', 1)
result_match = None
if len(result_section) == 2:
    result_match = re.search(r'^- `([^`]+)`$', result_section[1], re.MULTILINE)

for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
