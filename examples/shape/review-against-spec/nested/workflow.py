"""Mock agent for the review-against-spec shape pair.

Stands in for a real reviewer so both shapes run with no credentials. The work
is keyed by task title, not id, so the flat and the nested shape — the same
review authored two ways — find the same things, and the only difference left
between their runs is the shape.

The nested shape's reading is a supervisor: on its first visit it briefs each
check under it, and it writes its result, the verdict, only on the visit that
finds every check finished. On every other visit it returns without one and the
engine takes the self-loop that releases the next check. The hold and release
are the engine's, not the mock's.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

READING = 'Read pull request 412: a streaming CSV writer behind the existing export command.'
VERDICT = 'Request changes: G1 fails, a 1 GB export peaks at 340 MB; R1 and N1 hold.'

RESULTS = {
    'Read pull request 412': READING,
    'Check R1, exports stream their rows':
        'R1 holds: rows go to the file through a 64 KB buffer, never a whole file.',
    'Check G1, a 1 GB export stays under 200 MB':
        'G1 fails: the 1 GB fixture peaks at 340 MB; the writer buffers a page before flushing.',
    'Check N1, no new export format':
        'N1 holds: `--format` still accepts csv and json, nothing else.',
    'Write the verdict': VERDICT,
    'Review pull request 412 against its spec': VERDICT,
}

# What each check is owed from the reading, by the check's title. The flat shape
# hands the whole reading to every check as an export; the nested reading
# briefs each check with its own part of it.
BRIEFS = {
    'Check R1, exports stream their rows':
        'The writer is `export/csv_stream.rs`; look at `flush_page`.\n',
    'Check G1, a 1 GB export stays under 200 MB':
        'Run the 1 GB fixture under `/usr/bin/time -v`; the page size is 10 000 rows.\n',
    'Check N1, no new export format':
        '`cli/export.rs` is the only place a format is parsed.\n',
}

# The flat shape's handoffs: the reading every check consumes, and the finding
# each check owes the verdict. The nested shape needs neither — the checks are
# briefed by the reading above them, and their results are the verdict's input.
EXPORTS = {
    'Read pull request 412': {'reading': READING + '\n\n' + ''.join(BRIEFS.values())},
}
EXPORTS.update({check: {'finding': RESULTS[check] + '\n'} for check in BRIEFS})


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


def result(summary):
    if not result_match:
        return
    path = pathlib.Path(result_match.group(1))
    write(path if path.is_absolute() else root / path, summary + '\n')


for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)

def children():
    """The (local id, title) of every child written under this task in its file."""
    file_match = re.search(r"this task's file `([^`]+)`", prompt)
    text = pathlib.Path(file_match.group(1)).read_text(encoding='utf-8')
    local = re.escape(task.split('.', 1)[1])
    return re.findall(r'^#+ Task %s\.(\d+): (.+)$' % local, text, re.MULTILINE)


if os.environ.get('RHEI_STATE') == 'supervising':
    # The reading. Brief every check once, then finish only on the visit that
    # finds each of them with a result.
    checks = children()
    for local, check in checks:
        brief = root / 'runtime' / 'supervise' / ('%s.%s.md' % (task, local))
        if not brief.exists():
            write(brief, BRIEFS[check.strip()])
    if checks and all(
            (root / 'runtime' / 'results' / ('%s.%s.md' % (task, local))).is_file()
            for local, _ in checks):
        result(RESULTS[title])
else:
    result(RESULTS.get(title, 'Did %s.' % title))
