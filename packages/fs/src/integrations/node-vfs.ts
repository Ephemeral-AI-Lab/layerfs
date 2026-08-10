export interface NodeVfsFilesystemBridge {
  readRangeSync(path: string, position: number, length: number): Uint8Array;
  statSync(path: string): unknown;
  stageWriteSync(path: string, content: Uint8Array, position: number): void;
  commitVisibleSync(path: string): void;
  abortWriteSync(path: string): void;
}

