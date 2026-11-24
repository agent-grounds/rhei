"""Credential-free agent; the settings launcher reads its prompt from stdin.

Discussion artifacts are produced by workflow.py callbacks. This mock supplies
the task result needed for both terminal entries. §FS-rhei-states.3.3.1
"""

import pathlib
import re
import sys


def main(prompt):
    """Write the Result section's path when this state can finish the task.

    The collect state has no Result section and needs no result yet.
    Judge and apply both supply one. §FS-rhei-states.3.3.1
    """
    _, separator, section = prompt.rpartition('\n## Result\n')
    if not separator:
        return
    section = section.split('\n## ', 1)[0]
    match = re.search(r'^- `([^`]+)`$', section, re.MULTILINE)
    if not match:
        sys.exit('prompt Result section has no result path')
    path = pathlib.Path(match.group(1))
    if not path.is_absolute():
        path = pathlib.Path(__file__).resolve().parent / path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('Mock agent finished; workflow.py callbacks record the discussion and application.\n',
                    encoding='utf-8')
