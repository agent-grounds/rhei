"""Mock agent and wait program for the waiting-on-a-person shape pair.

Stands in for a real agent, and for the program that reads an issue's
comments, so both shapes run with no credentials and touch no forge. The
author's reply is already in `forge/issue-412.md`, the stand-in issue, so the
first look finds it; take the reply out and the wait exits 75 until the poll
budget runs out. The work is keyed by task title, not id.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

RESULTS = {
    'Ask the author which encoding to use':
        'Asked the author on issue 412 which encoding the export should write.',
    'Implement the export':
        'The export writes UTF-8 with a byte-order mark, as the author answered.',
}

ANSWERED = 'The author answered: UTF-8 with a byte-order mark, so Excel opens the file.'


def reply():
    """The author's reply on the stand-in issue, or exit 75 to look again later."""
    issue = pathlib.Path('forge') / 'issue-412.md'
    found = re.search(r'^author: (.+)$', issue.read_text(encoding='utf-8'), re.MULTILINE)
    if not found:
        sys.exit(75)
    return found.group(1).strip() + '\n'


# The wait's export is the reply itself, read when the wait runs.
PROGRAM_EXPORTS = {
    ('wait', 'Ask the author which encoding to use'): lambda: {'answer': reply()},
    ('wait', "Wait for the author's answer"): lambda: {'answer': reply()},
}
PROGRAM_RESULTS = {
    ('wait', 'Ask the author which encoding to use'): ANSWERED,
    ('wait', "Wait for the author's answer"): ANSWERED,
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
        write(path, text)
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
