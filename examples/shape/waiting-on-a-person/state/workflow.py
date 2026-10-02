"""Mock agent and wait program for the waiting-on-a-person shape pair.

Stands in for a real agent, and for the program that reads an issue's
comments, so both shapes run with no credentials and touch no forge. The
author's reply is already in `forge/issue-412.md`, the stand-in issue, so the
first look finds it; take the reply out and every look keeps an answer saying
no reply came and exits 75, until the poll budget ends the wait. The work is
keyed by task title, not id.

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

# What the export does when the answer it consumes says no reply came.
UNANSWERED_RESULTS = {
    'Implement the export':
        'No answer came on issue 412, so the export writes plain UTF-8 until one does.',
}

ANSWERED = 'The author answered: UTF-8 with a byte-order mark, so Excel opens the file.'
NO_REPLY = 'No reply came on issue 412.'

EXPORTS = {}

STATE_OUTPUTS = {}


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def reply():
    """The author's reply on the stand-in issue, or None when there is none yet."""
    issue = pathlib.Path('forge') / 'issue-412.md'
    found = re.search(r'^author: (.+)$', issue.read_text(encoding='utf-8'), re.MULTILINE)
    return found.group(1).strip() + '\n' if found else None


def wait():
    """One look. A reply becomes the task's `answer` and ends the wait with exit
    0. No reply yet keeps an answer that says so and exits 75, another look after
    the interval: whichever look the poll budget ends the wait on has left the
    `answer` its task owes, and the result counts the looks."""
    found = reply()
    looks = int(os.environ['RHEI_VISIT_COUNT'])
    answer, summary = (found, ANSWERED) if found else (
        NO_REPLY + '\n', 'The author had not replied on issue 412 after %d looks.' % looks)
    write(pathlib.Path('runtime') / 'exports' / os.environ['RHEI_TASK_ID'] / 'answer.md', answer)
    write(os.environ['RHEI_RESULT_PATH'], summary + '\n')
    sys.exit(0 if found else 75)


def prompt_arg():
    """The authoritative autonomous context delivered by `prompt_flag`."""
    args = sys.argv[1:]
    for index, arg in enumerate(args[:-1]):
        if arg == '--prompt':
            return args[index + 1]
    return ''


if sys.argv[1:3] == ['--program', 'wait']:
    wait()

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
    if NO_REPLY in prompt:
        summary = UNANSWERED_RESULTS.get(title, summary)
    write(path if path.is_absolute() else root / path, summary + '\n')
