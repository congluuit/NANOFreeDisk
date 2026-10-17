// Home's background AI-leftovers check runs once per app session.

let checked = false;

export const aiChecked = (): boolean => checked;

export function markAiChecked(): void {
  checked = true;
}
