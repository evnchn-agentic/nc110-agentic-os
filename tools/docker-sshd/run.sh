#!/bin/sh
# Start the throwaway sshd container ([config name], default sshd_config; sshd_config.demo gives a real shell; sshd_config.load streams ~2 MB) on 192.168.50.95:2223 (log: docker logs nc110-sshd). Stop: docker rm -f nc110-sshd
docker rm -f nc110-sshd >/dev/null 2>&1
mkdir -p /tmp/nc110-sshd-keys && cp test-keys/host_ed25519 test-keys/authorized_keys /tmp/nc110-sshd-keys/
# also trust the installed agent OS (its key lives outside the repo)
[ -f "$HOME/.nc110/agentos.key" ] && python3 -c "import base64,struct,sys; k=open(sys.argv[1],'rb').read()[40:72]; print('ssh-ed25519 '+base64.b64encode(struct.pack('>I',11)+b'ssh-ed25519'+struct.pack('>I',32)+k).decode()+' nc110-agentos')" "$HOME/.nc110/agentos.key" >> /tmp/nc110-sshd-keys/authorized_keys
docker run -d --name nc110-sshd -p 192.168.50.95:2223:22 -v "$PWD/tools/docker-sshd/${1:-sshd_config}:/etc/ssh/sshd_config.nc110:ro" \
  -v /tmp/nc110-sshd-keys:/keys:ro alpine:3.20 sh -c 'apk add -q --no-cache openssh >/dev/null && adduser -D -s /bin/sh evnchn \
  && passwd -u evnchn >/dev/null 2>&1; printf "\\n  Hello from a Linux container on the M4 Mac,\\n  reached over SSH from the NC110 bare-metal netbook.\\n\\n" > /etc/motd; ssh-keygen -A >/dev/null; exec /usr/sbin/sshd -D -e -f /etc/ssh/sshd_config.nc110'
