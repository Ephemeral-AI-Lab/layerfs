import { EphemeralFS, type EphemeralFS as EphemeralFilesystem } from "@ephemeralai/fs";
import {
  openCloudflareSqlite,
  type DurableObjectSQLiteStorage,
} from "@ephemeralai/fs-sqlite-cloudflare";
import { DurableObject } from "cloudflare:workers";

interface Env {
  readonly FILESYSTEM: DurableObjectNamespace<FilesystemObject>;
  /** Set as a hosted-preview secret at M9; intentionally absent in local M6. */
  readonly EFS_PREVIEW_TOKEN?: string;
}

type PreviewCommand =
  | { readonly operation: "mkdir"; readonly path: string; readonly recursive?: boolean }
  | {
      readonly operation: "readdir";
      readonly path: string;
      readonly limit?: number;
      readonly startAfter?: string;
    }
  | { readonly operation: "stat" | "lstat"; readonly path: string }
  | { readonly operation: "chmod"; readonly path: string; readonly mode: number }
  | {
      readonly operation: "link";
      readonly source: string;
      readonly destination: string;
    }
  | { readonly operation: "symlink"; readonly target: string; readonly path: string }
  | { readonly operation: "readlink"; readonly path: string }
  | {
      readonly operation: "rename";
      readonly source: string;
      readonly destination: string;
    }
  | { readonly operation: "unlink"; readonly path: string }
  | {
      readonly operation: "rm";
      readonly path: string;
      readonly recursive?: boolean;
      readonly force?: boolean;
    }
  | {
      readonly operation: "writeRange";
      readonly path: string;
      readonly offset: number;
      readonly bytes: readonly number[];
    }
  | {
      readonly operation: "replaceRange";
      readonly path: string;
      readonly offset: number;
      readonly deleteLength: number;
      readonly bytes: readonly number[];
    }
  | { readonly operation: "truncate"; readonly path: string; readonly size: number }
  | { readonly operation: "branchCreate"; readonly branchId: string }
  | {
      readonly operation: "branchWrite";
      readonly branchId: string;
      readonly path: string;
      readonly bytes: readonly number[];
    }
  | {
      readonly operation: "branchPublish";
      readonly branchId: string;
      readonly operationId?: string;
    }
  | { readonly operation: "branchDiscard"; readonly branchId: string }
  | { readonly operation: "snapshot"; readonly maxBatches?: number }
  | {
      readonly operation: "collect";
      readonly runId: string;
      readonly maxBatches?: number;
    }
  | {
      readonly operation: "verify";
      readonly cursor?: string;
      readonly maxEntities?: number;
    }
  | { readonly operation: "runtimeIdentity" }
  | { readonly operation: "abortForRestart" };

function plainStat(stat: Awaited<ReturnType<EphemeralFilesystem["stat"]>>) {
  return {
    id: stat.id,
    name: stat.name,
    type: stat.type,
    mode: stat.mode,
    size: stat.size,
    nlink: stat.nlink,
    mtimeMs: stat.mtimeMs,
    ctimeMs: stat.ctimeMs,
    birthtimeMs: stat.birthtimeMs,
  };
}

function bytes(values: readonly number[]): Uint8Array {
  if (
    !Array.isArray(values) ||
    values.some((value) => !Number.isInteger(value) || value < 0 || value > 255)
  )
    throw new RangeError("preview byte array contains a non-byte value");
  return Uint8Array.from(values);
}

export class FilesystemObject extends DurableObject<Env> {
  #filesystem: Promise<EphemeralFilesystem> | undefined;
  readonly #instanceNonce = crypto.randomUUID();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
  }

  async fetch(request: Request): Promise<Response> {
    const filesystem = await this.#ready();
    const url = new URL(request.url);
    const segments = url.pathname.split("/").filter(Boolean);
    const surface = segments[1] ?? "status";
    if (surface === "file") {
      const pathname = `/${segments.slice(2).map(decodeURIComponent).join("/")}`;
      if (request.method === "GET")
        return new Response(await filesystem.readStream(pathname), {
          headers: { "content-type": "application/octet-stream" },
        });
      if (request.method === "PUT") {
        const declared = Number(request.headers.get("content-length"));
        if (
          request.body === null ||
          !Number.isSafeInteger(declared) ||
          declared < 0 ||
          declared > filesystem.capabilities.storage.maxFileBytes
        )
          return new Response("a finite Content-Length is required", { status: 411 });
        await filesystem.writeFile(pathname, request.body, { maxBytes: declared });
        return Response.json({ ok: true, bytes: declared });
      }
      return new Response("method not allowed", { status: 405 });
    }
    if (surface === "rpc" && request.method === "POST") {
      const command = (await request.json()) as PreviewCommand;
      return Response.json(await this.#execute(filesystem, command));
    }
    return Response.json({
      capabilities: filesystem.capabilities,
      storage: await filesystem.maintenance.snapshotStorage(),
      runtime: await this.runtimeIdentity(),
    });
  }

  async writeText(pathname: string, value: string): Promise<"ok"> {
    const filesystem = await this.#ready();
    await filesystem.writeFile(pathname, value);
    return "ok";
  }

  async readText(pathname: string): Promise<string> {
    const filesystem = await this.#ready();
    return filesystem.readFile(pathname, { encoding: "utf8" });
  }

  async runtimeIdentity(): Promise<{
    readonly databaseSize: number;
    readonly instanceNonce: string;
  }> {
    return {
      databaseSize: this.ctx.storage.sql.databaseSize,
      instanceNonce: this.#instanceNonce,
    };
  }

  async #execute(
    filesystem: EphemeralFilesystem,
    command: PreviewCommand,
  ): Promise<unknown> {
    switch (command.operation) {
      case "mkdir":
        await filesystem.mkdir(command.path, { recursive: command.recursive });
        return { ok: true };
      case "readdir":
        return (
          await filesystem.readdir(command.path, {
            ...(command.limit === undefined ? {} : { limit: command.limit }),
            ...(command.startAfter === undefined
              ? {}
              : { startAfter: command.startAfter }),
          })
        ).map((entry) => ({ name: entry.name, type: entry.type }));
      case "stat":
        return plainStat(await filesystem.stat(command.path));
      case "lstat":
        return plainStat(await filesystem.lstat(command.path));
      case "chmod":
        await filesystem.chmod(command.path, command.mode);
        return { ok: true };
      case "link":
        await filesystem.link(command.source, command.destination);
        return { ok: true };
      case "symlink":
        await filesystem.symlink(command.target, command.path);
        return { ok: true };
      case "readlink":
        return { target: await filesystem.readlink(command.path) };
      case "rename":
        await filesystem.rename(command.source, command.destination);
        return { ok: true };
      case "unlink":
        await filesystem.unlink(command.path);
        return { ok: true };
      case "rm":
        await filesystem.rm(command.path, {
          recursive: command.recursive,
          force: command.force,
        });
        return { ok: true };
      case "writeRange":
        await filesystem.writeRange(command.path, command.offset, bytes(command.bytes));
        return { ok: true };
      case "replaceRange":
        await filesystem.replaceRange(
          command.path,
          command.offset,
          command.deleteLength,
          bytes(command.bytes),
        );
        return { ok: true };
      case "truncate":
        await filesystem.truncate(command.path, command.size);
        return { ok: true };
      case "branchCreate": {
        const branch = await filesystem.branches.create(command.branchId);
        try {
          return await branch.info();
        } finally {
          await branch.close();
        }
      }
      case "branchWrite": {
        const branch = await filesystem.branches.open(command.branchId);
        try {
          await branch.writeFile(command.path, bytes(command.bytes));
          return { ok: true };
        } finally {
          await branch.close();
        }
      }
      case "branchPublish": {
        const branch = await filesystem.branches.open(command.branchId);
        try {
          return await branch.publish(
            command.operationId === undefined
              ? {}
              : { operationId: command.operationId },
          );
        } finally {
          await branch.close();
        }
      }
      case "branchDiscard": {
        const branch = await filesystem.branches.open(command.branchId);
        try {
          return await branch.discard();
        } finally {
          await branch.close();
        }
      }
      case "snapshot":
        return filesystem.maintenance.snapshotStorage({
          maxBatches: command.maxBatches,
        });
      case "collect":
        return filesystem.maintenance.collectGarbage({
          runId: command.runId,
          maxBatches: command.maxBatches,
        });
      case "verify":
        return filesystem.maintenance.verify({
          cursor: command.cursor,
          maxEntities: command.maxEntities,
        });
      case "runtimeIdentity":
        return this.runtimeIdentity();
      case "abortForRestart":
        this.ctx.abort("M9 hosted-preview restart probe");
        throw new Error("Durable Object abort unexpectedly returned");
    }
  }

  #ready(): Promise<EphemeralFilesystem> {
    this.#filesystem ??= this.ctx.blockConcurrencyWhile(async () => {
      const database = await openCloudflareSqlite({
        storage: this.ctx.storage as DurableObjectSQLiteStorage,
      });
      return EphemeralFS.open({ database });
    });
    return this.#filesystem;
  }
}

export default {
  fetch(request: Request, env: Env): Promise<Response> {
    if (
      env.EFS_PREVIEW_TOKEN !== undefined &&
      request.headers.get("authorization") !== `Bearer ${env.EFS_PREVIEW_TOKEN}`
    )
      return Promise.resolve(new Response("unauthorized", { status: 401 }));
    const pathname = new URL(request.url).pathname;
    const objectName = pathname.split("/").filter(Boolean)[0] ?? "default";
    return env.FILESYSTEM.getByName(objectName).fetch(request);
  },
};
