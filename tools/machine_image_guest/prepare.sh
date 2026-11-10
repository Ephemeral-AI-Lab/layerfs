#!/bin/sh
set -eu
python3 -c 'import yaml'
install -m 755 /opt/layerfs-image/identity.py /usr/local/sbin/layerfs-image-identity
install -m 644 /opt/layerfs-image/identity.service /etc/systemd/system/layerfs-image-identity.service
install -m 644 /opt/layerfs-image/boot.service /etc/systemd/system/layerfs-image-boot.service
systemctl mask snapd.service snapd.socket snapd.seeded.service snapd.autoimport.service snapd.core-fixup.service snapd.recovery-chooser-trigger.service ModemManager.service apport.service udisks2.service
# Socket activation orders sockets.target before basic.target. An identity
# service with normal dependencies cannot also precede ssh.socket: that cycles.
systemctl disable ssh.socket
systemctl enable ssh.service layerfs-image-identity.service layerfs-image-boot.service
mkdir -p /etc/systemd/system/ssh.service.d
cat > /etc/systemd/system/ssh.service.d/layerfs-identity.conf <<'EOF'
[Unit]
Requires=layerfs-image-identity.service
After=layerfs-image-identity.service
EOF
touch /etc/cloud/cloud-init.disabled
mkdir -p /etc/netplan
rm -f /etc/netplan/*.yaml
cat > /etc/netplan/50-layerfs.yaml <<'EOF'
network:
  version: 2
  ethernets:
    lima:
      match:
        name: "e*"
      dhcp4: true
      dhcp6: false
      dhcp-identifier: mac
EOF
chmod 600 /etc/netplan/50-layerfs.yaml
netplan generate
# AppArmor stays active. LayerFS's normal preparation restricts user namespaces.
