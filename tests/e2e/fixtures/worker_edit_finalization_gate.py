"""Hold a real authoritative snapshot until sibling finalization publishes.

The C observer pauses the engine's read after state publication, while the
stable sidecar is held. A correct publisher either waits for terminal finalization
or reads only after acquiring that sidecar. It then sees the finalized link.
The old fixture reads completed state without the link, and overwrites the link
after the engine publishes it. No bytes or state changes are manufactured here.
"""
_original_read_text = pathlib.Path.read_text
_gate = pathlib.Path(__file__).parent
_editing = False


def _before_shared_edit():
    global _editing
    _editing = True


def _controlled_read_text(self, *args, **kwargs):
    text = _original_read_text(self, *args, **kwargs)
    if self.name == '01-shared.md' and _editing:
        sibling = text.split('### Task 2:', 1)[1]
        if '**State:** completed' in sibling and '> **Result:**' not in sibling:
            (_gate / 'stale-shared.md').write_text(text, encoding='utf-8')
            (_gate / 'snapshot-held').touch()
            deadline = time.monotonic() + 20
            while not (_gate / 'finalized').exists():
                if time.monotonic() > deadline:
                    raise RuntimeError('barrier timeout: finalized')
                time.sleep(0.01)
    return text


pathlib.Path.read_text = _controlled_read_text
