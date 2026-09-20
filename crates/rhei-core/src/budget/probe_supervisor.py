"""Private PID-1 supervisor and Unix-to-loopback relay. §FS-rhei-budgets.12"""
import json
import os
import selectors
import socket
import subprocess
import sys
import threading
import time

deadline = time.monotonic() + int(sys.argv[1]) / 1000
listener = socket.socket()
listener.bind(("127.0.0.1", 0))
listener.listen(1)
listener.settimeout(0.1)


def relay():
    while time.monotonic() < deadline:
        try:
            local, _ = listener.accept()
        except TimeoutError:
            continue
        with local, socket.socket(socket.AF_UNIX) as remote:
            remote.settimeout(max(0.001, deadline - time.monotonic()))
            remote.connect("/rhei/broker.sock")
            with selectors.DefaultSelector() as selector:
                selector.register(local, selectors.EVENT_READ, remote)
                selector.register(remote, selectors.EVENT_READ, local)
                active = True
                while active and time.monotonic() < deadline:
                    for key, _ in selector.select(0.1):
                        data = key.fileobj.recv(16384)
                        if not data:
                            active = False
                            break
                        key.data.settimeout(max(0.001, deadline - time.monotonic()))
                        key.data.sendall(data)


threading.Thread(target=relay, daemon=True).start()
with open("/rhei/launch.json", encoding="utf-8") as config:
    argv = json.load(config)
# The signed case configuration chooses how each pinned client consumes this
# endpoint. No host API key or host environment is forwarded.
endpoint = "http://127.0.0.1:%d/v1" % listener.getsockname()[1]
argv = [arg.replace("{broker_url}", endpoint) for arg in argv]
child = subprocess.Popen(argv, close_fds=True)
try:
    status = child.wait(timeout=max(0.001, deadline - time.monotonic()))
except subprocess.TimeoutExpired:
    status = 124
# Exiting the PID-namespace init destroys detached children as well.
os._exit(status if 0 <= status <= 255 else 125)
