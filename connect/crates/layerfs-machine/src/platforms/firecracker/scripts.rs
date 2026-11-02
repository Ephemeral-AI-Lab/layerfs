//! `firecracker` scripts: see `mod.rs`.

/// One line per missing thing, so the refusal can name each.
pub(super) const CHECK: &str = r#"test -c /dev/kvm || echo "/dev/kvm"
for f in bin/firecracker vmlinux rootfs.ext4 id_ed25519; do test -f "$1/$f" || echo "$1/$f"; done
for t in mkfs.ext4 ip flock ssh truncate; do command -v $t >/dev/null || echo "tool:$t"; done
true"#;

/// The data disk, made once; exit 17 when one is already there.
pub(super) const DISK: &str = r#"set -e
f="$1/disks/$2.ext4"
test ! -e "$f" || { echo "$f already exists" >&2; exit 17; }
mkdir -p "$1/disks" && truncate -s "$3" "$f.part" && mkfs.ext4 -q -F "$f.part" && mv "$f.part" "$f""#;

/// The VM's folder under a lock: network `n` (`$6`, or the first free one
/// for 0), a root filesystem copy and the config. Prints `n`.
pub(super) const CREATE: &str = r#"set -eu
root=$1 name=$2 disk=$3 cpus=$4 mem=$5 want=$6
d="$root/vms/$name"
mkdir -p "$root/vms"
exec 9>"$root/vms/.lock"
flock 9
test ! -e "$d" || { echo "$d already exists" >&2; exit 17; }
taken() { grep -qx "INDEX=$1" "$root"/vms/*/vm.env 2>/dev/null; }
if [ "$want" -gt 0 ]; then n=$want; ! taken "$n" || { echo "network $n is taken" >&2; exit 1; }
else n=1; while taken "$n"; do n=$((n+1)); done; fi
[ "$n" -le 250 ] || { echo "every network number is taken" >&2; exit 1; }
mkdir "$d"
printf 'INDEX=%s\nTAP=lfsfc%s\nHOST_IP=172.30.%s.1\nGUEST_IP=172.30.%s.2\n' "$n" "$n" "$n" "$n" > "$d/vm.env"
cp --sparse=always "$root/rootfs.ext4" "$d/rootfs.ext4"
mac=$(printf '06:00:AC:1E:%02X:02' "$n")
cat > "$d/config.json" <<EOF
{"boot-source":{"kernel_image_path":"$root/vmlinux","boot_args":"console=ttyS0 reboot=k panic=1 pci=off ip=172.30.$n.2::172.30.$n.1:255.255.255.252::eth0:off"},
 "drives":[{"drive_id":"rootfs","path_on_host":"$d/rootfs.ext4","is_root_device":true,"is_read_only":false},
  {"drive_id":"data","path_on_host":"$disk","is_root_device":false,"is_read_only":false}],
 "machine-config":{"vcpu_count":$cpus,"mem_size_mib":$mem,"huge_pages":"None"},
 "network-interfaces":[{"iface_id":"eth0","guest_mac":"$mac","host_dev_name":"lfsfc$n"}]}
EOF
echo "$n""#;

/// Boots the microVM (unless it runs), waits for its sshd (a readiness wait,
/// at most 60 s), and mounts its data disk at `/layerfs/data`. Its memory
/// uses 2 MiB huge pages when the host has enough free at this boot, else
/// none: a host's huge pages do not outlive its reboot.
pub(super) const BOOT_VM: &str = r#"set -eu
root=$1 d="$1/vms/$2"
. "$d/vm.env"
G="-o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5"
if ! { [ -f "$d/firecracker.pid" ] && kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null; }; then
  mem=$(sed -n 's/.*"mem_size_mib":\([0-9]*\).*/\1/p' "$d/config.json")
  huge=None
  [ "$(( $(awk '/^HugePages_Free/ {print $2}' /proc/meminfo) * 2 ))" -ge "$mem" ] && huge=2M
  sed -i "s/\"huge_pages\":\"[A-Za-z0-9]*\"/\"huge_pages\":\"$huge\"/" "$d/config.json"
  ip link show "$TAP" >/dev/null 2>&1 || ip tuntap add dev "$TAP" mode tap
  ip addr replace "$HOST_IP/30" dev "$TAP"
  ip link set "$TAP" up
  setsid nohup "$root/bin/firecracker" --no-api --config-file "$d/config.json" > "$d/firecracker.log" 2>&1 < /dev/null &
  echo $! > "$d/firecracker.pid"
fi
i=0
until ssh -i "$root/id_ed25519" $G "root@$GUEST_IP" true 2>/dev/null; do
  kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null || { echo "firecracker ended: $(tail -n 5 "$d/firecracker.log")" >&2; exit 1; }
  i=$((i+1)); [ "$i" -lt 120 ] || { echo "the microVM did not answer ssh within 60 s" >&2; exit 1; }
  sleep 0.5
done
ssh -i "$root/id_ed25519" $G "root@$GUEST_IP" 'mkdir -p /layerfs/data && { mountpoint -q /layerfs/data || mount -t ext4 /dev/vdb /layerfs/data; } && test "$(findmnt -n -o FSTYPE /layerfs/data)" = ext4'"#;

/// `running|stopped`, then the guest address; exit 3 when there is no such VM.
pub(super) const INSPECT: &str = r#"d="$1/vms/$2"
test -f "$d/vm.env" || exit 3
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then echo running; else echo stopped; fi
echo "$GUEST_IP""#;

/// Syncs and resets the guest (Firecracker exits on a reset), removes its
/// tap, and with `delete` its folder and data disk.
pub(super) const STOP: &str = r#"root=$1 d="$1/vms/$2"
test -f "$d/vm.env" || exit 0
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
  timeout 10 ssh -i "$root/id_ed25519" -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR "root@$GUEST_IP" 'sync; reboot -f' >/dev/null 2>&1 || true
  i=0; while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i+1)); done
  kill -0 "$pid" 2>/dev/null && kill -9 "$pid"
fi
rm -f "$d/firecracker.pid"
ip link del "$TAP" 2>/dev/null || true
if [ "${3:-}" = delete ]; then rm -rf "$d" "$1/disks/$2.ext4"; fi
true"#;
