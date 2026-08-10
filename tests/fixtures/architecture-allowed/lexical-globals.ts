export function useLexicalGlobalNames(
  window: number,
  global: number,
  self: number,
): number {
  const values = { window, global, self };
  return values.window + values.global + values.self;
}
