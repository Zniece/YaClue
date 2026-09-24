// The visible zero is a placeholder until the user enters an expression.
// When calculating an untouched field, make that displayed value real.
export function materializeDisplayedZero(field) {
  if (field.value.trim()) return false;
  field.value = "0";
  return true;
}
