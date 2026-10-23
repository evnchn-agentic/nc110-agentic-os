ListenAddress 127.0.0.1:2223
HostKey /path/to/nc110-agentic-os/test-keys/host_ed25519
HostKeyAlgorithms ssh-ed25519
KexAlgorithms curve25519-sha256
Ciphers chacha20-poly1305@openssh.com
PubkeyAcceptedAlgorithms ssh-ed25519
AuthorizedKeysFile /path/to/nc110-agentic-os/test-keys/authorized_keys
AuthenticationMethods publickey
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
StrictModes no
PermitRootLogin no
AllowUsers evnchn
ForceCommand /usr/bin/env -i PS1='nc110$ ' /bin/sh -i
PidFile /path/to/nc110-agentic-os/test-keys/sshd-m3.pid
LogLevel VERBOSE
