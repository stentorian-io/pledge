// Destructuring patterns

// Object destructuring
const obj = { a: 1, b: 2, c: 3 };
export const { a, b, c } = obj;

// Array destructuring
const arr = [1, 2, 3, 4, 5];
export const [first, second, ...rest] = arr;

// Nested destructuring
const nested = { outer: { inner: { value: 42 } } };
export const { outer: { inner: { value: nestedValue } } } = nested;

// Default values in destructuring
const partial = { x: 1 };
export const { x, y = 10 } = partial;

// Renamed destructuring
const original = { oldName: 'value' };
export const { oldName: newName } = original;

// Mixed destructuring
const complex = { items: [1, 2, 3], meta: { count: 3 } };
export const { items: [head], meta: { count } } = complex;

// Function parameter destructuring
export function withDestructuredParams({ name, age = 0 }, [first2, ...others]) {
    return { name, age, first: first2, others };
}

// Computed property destructuring
const key = 'dynamic';
const dynamic = { [key]: 'value' };
const { [key]: dynamicValue } = dynamic;
export { dynamicValue };

// Destructuring with rest
const { a: _, ...remaining } = { a: 1, b: 2, c: 3 };
export { remaining };
