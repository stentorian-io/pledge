// Simple module with various export types
export const CONSTANT = 42;

export function greet(name) {
    return `Hello, ${name}!`;
}

export class Calculator {
    add(a, b) {
        return a + b;
    }

    subtract(a, b) {
        return a - b;
    }
}

const internalValue = "internal";

export default function defaultExport() {
    return internalValue;
}
