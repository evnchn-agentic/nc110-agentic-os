#!/usr/bin/env python3
"""Turn the throwaway OpenSSH ed25519 keys in test-keys/ into raw bytes the kernel include_bytes!()s.

test-keys/client.key   72-byte multiboot key module: b"NC110K1\n" | ed25519 seed | ed25519 public key
                       (secret; gitignored dir). Loaded at boot by GRUB `module` / QEMU `-initrd`, NOT compiled in.
test-keys/host_pub.bin 32-byte ed25519 public key of the throwaway sshd's host key (the pin, compiled in)
"""
import base64, os, struct, sys

D = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "test-keys")

def rd(b, o):
    n = struct.unpack(">I", b[o:o + 4])[0]
    return b[o + 4:o + 4 + n], o + 4 + n

def pub_from_line(line):
    blob = base64.b64decode(line.split()[1])
    t, o = rd(blob, 0)
    k, o = rd(blob, o)
    assert t == b"ssh-ed25519" and len(k) == 32
    return k

def seed_from_private(text):
    b64 = "".join(l for l in text.splitlines() if not l.startswith("-----"))
    b = base64.b64decode(b64)
    assert b.startswith(b"openssh-key-v1\0")
    o = len(b"openssh-key-v1\0")
    cipher, o = rd(b, o); kdf, o = rd(b, o); _, o = rd(b, o)
    assert cipher == b"none" and kdf == b"none", "key must be unencrypted"
    nkeys = struct.unpack(">I", b[o:o + 4])[0]; o += 4
    assert nkeys == 1
    pubblob, o = rd(b, o)
    priv, o = rd(b, o)
    c1, c2 = struct.unpack(">II", priv[:8])
    assert c1 == c2
    p = 8
    t, p = rd(priv, p); pk, p = rd(priv, p); sk, p = rd(priv, p)
    assert t == b"ssh-ed25519" and len(sk) == 64 and sk[32:] == pk
    return sk[:32], pk

seed, pk = seed_from_private(open(os.path.join(D, "client_ed25519")).read())
assert pk == pub_from_line(open(os.path.join(D, "client_ed25519.pub")).read())
open(os.path.join(D, "client.key"), "wb").write(b"NC110K1\n" + seed + pk)
old = os.path.join(D, "client_seed.bin")  # M2 layout, compiled into the ELF: remove so nothing can include it
if os.path.exists(old):
    os.unlink(old)
open(os.path.join(D, "host_pub.bin"), "wb").write(pub_from_line(open(os.path.join(D, "host_ed25519.pub")).read()))
print("wrote client.key (module), host_pub.bin")
