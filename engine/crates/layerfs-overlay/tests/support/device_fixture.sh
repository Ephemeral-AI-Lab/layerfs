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
    overlay) proof_package=layerfs-overlay; proof_binary=device_capacity; proof_default=real_device_full_rolls_back_write_and_preserves_capture ;;
    daemon) proof_package=layerfs-daemon; proof_binary=device_capacity; proof_default=real_owner_reclaims_at_device_full_after_last_owner_without_a_cleanup_job ;;
    mounted) proof_package=layerfs-daemon; proof_binary=mounted_sqlite_full; proof_default=real_device_full_reports_enospc_and_normal_drain_completes ;;
    *) echo 'invalid proof package'; exit 1 ;;
esac
proof_test=${2:-$proof_default}
if [ "${3:-}" = ioerr ]; then
    [ "$proof_binary" != device_capacity ] || [ "$proof_package" = layerfs-overlay ]
    proof_shim=$proof_root/enospc-as-eio.so
    cc -shared -fPIC -Wall -Wextra -Werror engine/crates/layerfs-overlay/tests/enospc_as_eio.c -ldl -o "$proof_shim"
    export LAYERFS_PROOF_TRANSLATE_ENOSPC=1
    export LD_PRELOAD=$proof_shim
fi
timeout --signal=KILL 90 cargo test --manifest-path Cargo.toml --locked --offline -p "$proof_package" --test "$proof_binary" "$proof_test" -- --exact --ignored --nocapture --test-threads=1
printf 'OWNED PROOF COMPLETED; normal teardown follows\n'
