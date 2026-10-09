/**
 * @param {string} name
 * @param {string} age - changed from number to string
 * @returns {{ name: string, age: string }}
 */
export function createUser(name, age) {
  return { name, age };
}

/** @type {string} */
export const maxRetries = "3";
