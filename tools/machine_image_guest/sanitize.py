#!/usr/bin/python3
"""Remove seed-instance identity before sealing a powered-off image."""

import glob
import pathlib
import pwd
import shutil
import subprocess

import yaml


def main():
    config = yaml.safe_load(pathlib.Path("/mnt/lima-cidata/user-data").read_text())
    # Only users explicitly created for this seed are removed. The seed uses a
    # fixed builder account, never the human host's username or home directory.
    for user in config["users"]:
        if not isinstance(user, dict):
            raise TypeError("unexpected seed user entry")
        if user["name"] != "layerfs-image-build" or int(user["uid"]) != 1000:
            raise ValueError("refusing to seal an image with an unexpected seed user")
        account = pwd.getpwnam(user["name"])
        if account.pw_uid != 1000 or account.pw_dir != "/home/layerfs-image-build":
            raise ValueError("seed account identity differs")
        subprocess.run(["userdel", "--force", "--remove", user["name"]], check=True)
    for folder in [
        "/layerfs/state",
        "/opt/layerfs-image/__pycache__",
        "/var/lib/cloud",
        "/root/.ssh",
        "/var/log/journal",
        "/var/lib/dhcp",
        "/var/lib/systemd/network",
    ]:
        shutil.rmtree(folder, ignore_errors=True)
    patterns = [
        "/etc/ssh/ssh_host_*",
        "/home/*/.ssh",
        "/var/log/cloud-init*",
        "/var/lib/systemd/random-seed",
        "/etc/sudoers.d/90-cloud-init-users",
        "/etc/sudoers.d/90-layerfs-*",
        "/var/lib/dbus/machine-id",
    ]
    for pattern in patterns:
        for name in glob.glob(pattern):
            path = pathlib.Path(name)
            if path.is_dir() and not path.is_symlink():
                shutil.rmtree(path)
            else:
                path.unlink(missing_ok=True)
    pathlib.Path("/etc/machine-id").write_text("")
    pathlib.Path("/etc/hostname").write_text("layerfs-image\n")
    for log in pathlib.Path("/var/log").glob("*"):
        if log.is_file() and not log.is_symlink():
            log.write_bytes(b"")
    for folder in ["/tmp", "/var/tmp"]:
        for path in pathlib.Path(folder).iterdir():
            if path.is_dir() and not path.is_symlink():
                shutil.rmtree(path)
            else:
                path.unlink(missing_ok=True)
    if list(pathlib.Path("/etc/ssh").glob("ssh_host_*")) or pathlib.Path("/layerfs/state").exists():
        raise ValueError("image sanitation failed")
    subprocess.run(["sync"], check=True)


if __name__ == "__main__":
    main()
