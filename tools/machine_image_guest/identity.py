#!/usr/bin/python3
"""Initialize one private Lima instance from its own read-only cidata."""

import os
import pathlib
import pwd
import re
import subprocess

import yaml


def initialize(meta, config):
    hostname = meta["local-hostname"]
    if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9.-]{0,252}", hostname):
        raise ValueError("invalid instance hostname")
    users = config["users"]
    for user in users:
        if not isinstance(user, dict):
            raise TypeError("prepared images require explicit cidata users")
        name, uid = user["name"], int(user["uid"])
        if not re.fullmatch(r"[a-z_][a-z0-9_-]{0,31}", name) or name == "root" or uid <= 0:
            raise ValueError("invalid instance user")
        if not user["homedir"].startswith("/home/") or ".." in pathlib.Path(user["homedir"]).parts:
            raise ValueError("invalid user home")
        if not user["shell"].startswith("/") or not user["ssh-authorized-keys"]:
            raise ValueError("missing instance shell or SSH keys")
    pathlib.Path("/etc/hostname").write_text(hostname + "\n")
    subprocess.run(["hostname", hostname], check=True)
    hosts = pathlib.Path("/etc/hosts")
    lines = [line for line in hosts.read_text().splitlines() if not line.startswith("127.0.1.1")]
    hosts.write_text("\n".join(lines) + "\n127.0.1.1 " + hostname + "\n")
    for user in users:
        name, uid = user["name"], int(user["uid"])
        try:
            account = pwd.getpwnam(name)
        except KeyError:
            subprocess.run(
                ["useradd", "-m", "-U", "-u", str(uid), "-d", user["homedir"], "-s", user["shell"], name], check=True
            )
            account = pwd.getpwnam(name)
        if account.pw_uid != uid or account.pw_dir != user["homedir"]:
            raise ValueError("existing user differs from private instance cidata")
        subprocess.run(["usermod", "-L", name], check=True)
        ssh = pathlib.Path(account.pw_dir) / ".ssh"
        ssh.mkdir(mode=0o700, parents=True, exist_ok=True)
        keyfile = ssh / "authorized_keys"
        keyfile.write_text("\n".join(user["ssh-authorized-keys"]) + "\n")
        for file, mode in [(ssh, 0o700), (keyfile, 0o600)]:
            os.chown(file, account.pw_uid, account.pw_gid)
            os.chmod(file, mode)
        sudo = pathlib.Path("/etc/sudoers.d/90-layerfs-" + name)
        sudo.write_text(name + " ALL=(ALL) NOPASSWD:ALL\n")
        sudo.chmod(0o440)
    subprocess.run(["ssh-keygen", "-A"], check=True)


def main():
    cidata = pathlib.Path("/mnt/lima-cidata")
    cidata.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "mount",
            "-o",
            "ro,mode=0700,dmode=0700,overriderockperm,exec,uid=0",
            "/dev/disk/by-label/cidata",
            str(cidata),
        ],
        check=True,
    )
    initialize(yaml.safe_load((cidata / "meta-data").read_text()), yaml.safe_load((cidata / "user-data").read_text()))


if __name__ == "__main__":
    main()
