// Windows paths stay on the caller; LayerFS's paths belong to one WSL
// distribution. Copied from the dsh Host's windows/paths.js (#428); a Windows
// path of Init's folder is translated by the worker itself (worker/server.js).
import { execFile } from "node:child_process";
import { posix } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
export const wslArgs = (wsl, args) => ["--distribution", wsl.distribution, "--user", wsl.user, "--exec", ...args];
export const wsl = (settings, args, options = {}) => execute("wsl.exe", wslArgs(settings.wsl, args), { windowsHide: true, ...options });

export function absoluteLinux(path, label) {
  if (typeof path !== "string" || !path.startsWith("/") || path.includes("\0") || path.includes("\\"))
    throw new Error(`${label} must be an absolute Linux path in the selected WSL distribution`);
  return posix.normalize(path);
}

export function projectHome(wsl, name) {
  if (!/^[a-z0-9][a-z0-9_.-]{0,62}$/.test(name)) throw new Error("invalid project name");
  const root = absoluteLinux(wsl.home, "wsl.home"), path = posix.join(root, name);
  if (posix.dirname(path) !== root) throw new Error("project escapes its WSL home");
  return path;
}
