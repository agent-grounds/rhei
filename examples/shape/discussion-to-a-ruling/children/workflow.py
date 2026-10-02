"""Mock agent for the children shape of the discussion-to-a-ruling pair.

Stands in for real participant, judge and coding agents so the example runs
with no credentials. The work is keyed by task title, not id; the first line
of each result is the one-line summary every later Plan History shows for the
task.
"""

import os
import pathlib
import re
import sys

RULING = ('Rebase-merge a pull request whose every commit builds; squash one whose '
          'commits do not.')

RESULTS = {
    'Round 1: claude\'s position':
        'Squash every pull request: one commit per change keeps history readable.',
    'Round 1: codex\'s position':
        'Merge commits only: a squash loses the commits a bisect needs.',
    'Round 1: judge the round': 'No ruling yet: the positions are opposite; another round.',
    'Round 2: claude\'s position':
        'codex is right about bisect: keep the commits when each one builds.',
    'Round 2: codex\'s position':
        'Agreed with claude: squash the ones whose commits do not build on their own.',
    'Round 2: judge the round': 'Converged: ' + RULING,
    'Rule on the merge policy': 'Ruled in round 2: ' + RULING[0].lower() + RULING[1:],
    'Apply the merge policy':
        'Applied the ruling: the merge queue rebase-merges a pull request whose every '
        'commit builds, and squashes the rest.',
}

EXPORTS = {
    'Rule on the merge policy': {'ruling': RULING + '\n'},
}

STATE_OUTPUTS = {}

PROGRAM_WRITES = {}

PROGRAM_EXPORTS = {}

PROGRAM_RESULTS = {}


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def title_of(local):
    """A program has no prompt: its task's title is read from the plan."""
    plan = pathlib.Path(os.environ['RHEI_PLAN_PATH'])
    root = plan if plan.is_dir() else plan.parent
    for path in sorted(root.glob('tasks/*.md')):
        found = re.search(r'^#+ Task %s: (.+)$' % re.escape(local),
                          path.read_text(encoding='utf-8'), re.MULTILINE)
        if found:
            return found.group(1).strip()
    return local


def program(step):
    """A program state: it writes what its step makes, keyed by the task."""
    task = os.environ['RHEI_TASK_ID']
    title = title_of(os.environ['RHEI_TASK_ID_LOCAL'])
    for path, text in PROGRAM_WRITES.get(step, {}).items():
        write(path.format(task=task), text() if callable(text) else text)
    exports = PROGRAM_EXPORTS.get((step, title), {})
    for name, text in (exports() if callable(exports) else exports).items():
        write(pathlib.Path('runtime') / 'exports' / task / (name + '.md'), text)
    summary = PROGRAM_RESULTS.get((step, title))
    if callable(summary):
        summary = summary()
    if summary:
        write(os.environ['RHEI_RESULT_PATH'], summary + '\n')


def prompt_arg():
    """The authoritative autonomous context delivered by `prompt_flag`."""
    args = sys.argv[1:]
    for index, arg in enumerate(args[:-1]):
        if arg == '--prompt':
            return args[index + 1]
    return ''


if sys.argv[1:2] == ['--program']:
    program(sys.argv[2])
    sys.exit(0)

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
for pattern, text in STATE_OUTPUTS.get(os.environ.get('RHEI_STATE'), {}).items():
    write(root / pattern.format(task=task), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
