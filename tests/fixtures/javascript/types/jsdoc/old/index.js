/**
 * @param {string} name
 * @param {number} age
 * @returns {{ name: string, age: number }}
 */
export function createUser(name, age) {
  return { name, age };
}

/** @type {number} */
export const maxRetries = 3;
