#!/usr/bin/env python3
"""Has every JNA callback in the generated Kotlin bindings keep its thread attached.

    keep-attached.py <generated .kt file>...

JNA attaches a native thread to the VM for a callback and, by default,
detaches it again on return, and it remembers in its own thread storage that
the thread is one it attached. When WebRTC later attaches the same thread for
itself (to let go of a picture the phone's decoder made, which is a Java
buffer), it notes that in its own thread storage; and the next callback on
that thread, which JNA still thinks of as its own, detaches it from under
WebRTC. WebRTC then finds a JNIEnv in its storage for a thread that is not
attached and aborts the process ("TLS has a JNIEnv* but not attached?"). That
took a call down within a second of unfolding a Fold6, where the bigger
picture went to the hardware decoder.

So each callback object the generator writes registers a
CallbackThreadInitializer (CoreNative.keepAttached) as it is made, before JNA
first sees it: a thread JNA attaches then stays attached until it ends, and
WebRTC, finding it attached, never keeps an env of its own for it.

Fails if the generated code no longer looks as expected, rather than letting
a callback through without it.
"""

import re
import sys

CALL = "init { app.tomoshibi.core.CoreNative.keepAttached(this) }"

for path in sys.argv[1:]:
    text = open(path, encoding="utf-8").read()
    if CALL in text:
        sys.exit(f"{path}: already patched; generate the bindings again first")
    callbacks = set(re.findall(r"^internal interface (\w+) : com\.sun\.jna\.Callback \{", text, re.M))
    if not callbacks:
        sys.exit(f"{path}: no JNA callback interfaces found")
    # Every object implementing one of them, at any depth: the vtables'
    # methods, frees and clones, and the futures' continuation.
    pattern = re.compile(r"^(?P<indent>[ \t]*)internal object (?P<name>`?\w+`?)\s*:\s*(?P<type>\w+) \{\n", re.M)
    patched = 0

    def add(match):
        global patched
        if match.group("type") not in callbacks:
            return match.group(0)
        patched += 1
        return match.group(0) + match.group("indent") + "    " + CALL + "\n"

    text = pattern.sub(add, text)
    implemented = len(re.findall(r"internal object `?\w+`?\s*:\s*(?:" + "|".join(sorted(callbacks)) + r")\b", text))
    if patched == 0 or patched != implemented:
        sys.exit(f"{path}: patched {patched} of {implemented} callback objects")
    open(path, "w", encoding="utf-8").write(text)
    print(f"{path}: {patched} callbacks keep their threads attached")
