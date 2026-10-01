"""Mock agent for the reproducer shape pair.

Stands in for a real coding agent so both shapes run with no credentials. The
work is keyed by task title, not id, so the flat and the nested shape — the
same work authored two ways — write the same results and the same export, and
the only difference left between their runs is the shape.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import pathlib
import re
import sys

RESULTS = {
    'Triage the overflow report':
        'Issue 87 is real and new: no earlier report, and its input panics the parser.',
    'Search for a duplicate':
        'No duplicate: the nearest report, issue 52, is about float literals.',
    'Reproduce the overflow':
        'Reproduced: a 20-digit literal panics the parser; the script is the `reproducer` export.',
    'Fix the parser':
        'The parser rejects the literal with E0412 instead of panicking; the reproducer exits 1.',
    'Run the gate':
        'Gate green: 412 tests pass and the reproducer exits 1 with E0412.',
}

EXPORTS = {
    'Reproduce the overflow': {
        'reproducer':
            "printf 'let x = 18446744073709551616;\\n' | parser --check -\n\n"
            'Panics today with `attempt to add with overflow`. Fixed means exit 1\n'
            'with `E0412 integer literal out of range`.\n',
    },
}


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
