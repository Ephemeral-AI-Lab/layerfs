#!/bin/sh
# Owning resource proof fixture only; no performance or aggregate gate.
set -eu
proof_root=$(mktemp -d /tmp/layerfs-307-device.XXXXXX)
proof_image=$proof_root/device.img
proof_mount=$proof_root/mount
proof_loop=
cleanup() {
    if mountpoint -q "$proof_mount"; then
        if ! umount "$proof_mount"; then echo 'FAILED owned unmount'; exit 1; fi
    fi
    if [ -n "$proof_loop" ]; then
        proof_backing=$(losetup -ln -O BACK-FILE "$proof_loop")
        if [ "$proof_backing" != "$proof_image" ]; then echo 'FAILED loop ownership'; exit 1; fi
        losetup -d "$proof_loop"
    fi
    rm -r "$proof_root"
    printf 'OWNED TEARDOWN unmounted=true loop_detached=true artifacts_removed=true\n'
}
trap cleanup EXIT
mkdir "$proof_mount"
truncate -s 512M "$proof_image"
mkfs.ext4 -q -F -m 0 "$proof_image"
proof_loop=$(losetup --find --show "$proof_image")
proof_backing=$(losetup -ln -O BACK-FILE "$proof_loop")
[ "$proof_backing" = "$proof_image" ]
mount -t ext4 "$proof_loop" "$proof_mount"
export LAYERFS_PROOF_DEVICE_ROOT=$proof_mount
printf 'OWNED DEVICE image=%s loop=%s mount=%s\n' "$proof_image" "$proof_loop" "$proof_mount"
case "${1:-overlay}" in
    overlay) proof_package=layerfs-overlay ;;
    daemon) proof_package=layerfs-daemon ;;
    *) echo 'invalid proof package'; exit 1 ;;
esac
timeout --signal=KILL 120 cargo test --manifest-path core/Cargo.toml --locked -p "$proof_package" --test device_capacity -- --ignored --nocapture
printf 'OWNED PROOF COMPLETED; normal teardown follows\n'
