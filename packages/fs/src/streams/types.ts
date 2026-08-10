/** Bounded stream control state; payload buffers remain owned by the active operation. */
export interface StreamSnapshot { readonly leaseId: string; readonly expiresAtMs: number }
