export function add(left, right) {
  return left + right;
}

export function total(values) {
  return values.reduce((sum, value) => add(sum, value), 0);
}
