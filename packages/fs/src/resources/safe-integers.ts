export const MAX_SAFE_INTEGER = Number.MAX_SAFE_INTEGER;

export function checkedInteger(
  value: number,
  name: string,
  maximum = MAX_SAFE_INTEGER,
): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > maximum)
    throw new RangeError(`${name} must be a safe integer in [0, ${maximum}]`);
  return value;
}

export function checkedAdd(left: number, right: number, name = "sum"): number {
  checkedInteger(left, `${name}.left`);
  checkedInteger(right, `${name}.right`);
  return checkedInteger(left + right, name);
}

export function checkedMultiply(left: number, right: number, name = "product"): number {
  checkedInteger(left, `${name}.left`);
  checkedInteger(right, `${name}.right`);
  return checkedInteger(left * right, name);
}
