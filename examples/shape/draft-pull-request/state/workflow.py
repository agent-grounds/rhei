"""Mock agent and opening program for the draft-pull-request shape pair.

Stands in for a real coding agent, and for the program that pushes a branch
and opens a pull request, so both shapes run with no credentials and touch no
forge: opening appends the pull request to `forge/pulls.md`, the stand-in
forge, and numbers it from what is already there. The work is keyed by task
title, not id.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

RESULTS = {
    'Write the spec and the failing test':
        'Spec point 3 and the failing test `streams_rows` committed on fix/issue-87.',
    'Implement the change':
        'Implemented the streaming export on the draft pull request; `streams_rows` passes.',
}


def opened():
    """Open the draft pull request on the stand-in forge, and say which it is."""
    pulls = pathlib.Path('forge') / 'pulls.md'
    listed = pulls.read_text(encoding='utf-8')
    number = 411 + listed.count('\n- ')
    with pulls.open('a', encoding='utf-8', newline='') as handle:
        handle.write('- #%d draft, from fix/issue-87\n' % number)
    return 'draft pull request #%d, from fix/issue-87\n' % number


def pr():
    return {'pr': opened()}


PROGRAM_EXPORTS = {
    ('opening', 'Write the spec and the failing test'): pr,
    ('opening', 'Open the draft pull request'): pr,
}


def committed():
    return 'Spec and failing test committed; ' + exported()


def exported():
    task = os.environ['RHEI_TASK_ID']
    text = (pathlib.Path('runtime') / 'exports' / task / 'pr.md').read_text(encoding='utf-8')
    return text.strip().replace('draft pull request', 'draft PR') + '.'


PROGRAM_RESULTS = {
    ('opening', 'Write the spec and the failing test'): committed,
    ('opening', 'Open the draft pull request'): lambda: 'Opened ' + exported(),
}

EXPORTS = {}

STATE_OUTPUTS = {}

PROGRAM_WRITES = {}


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
