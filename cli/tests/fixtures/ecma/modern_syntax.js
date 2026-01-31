// Modern JavaScript syntax features

// Optional chaining
export const optionalChain = (obj) => obj?.nested?.value;

// Nullish coalescing
export const nullishCoalesce = (val) => val ?? 'default';

// Logical assignment operators
export function logicalAssignment() {
    let a = null;
    let b = 0;
    let c = 'defined';

    a ||= 'fallback';
    b &&= 10;
    c ??= 'unused';

    return { a, b, c };
}

// Numeric separators
export const billion = 1_000_000_000;
export const bytes = 0xFF_FF_FF_FF;
export const binary = 0b1010_1010;

// BigInt literals
export const bigNumber = 9007199254740991n;
export const bigHex = 0xFFFFFFFFFFFFFFFFn;

// Private class fields
export class Counter {
    #count = 0;

    increment() {
        return ++this.#count;
    }

    get value() {
        return this.#count;
    }
}

// Static blocks
export class Initialized {
    static data;

    static {
        this.data = { initialized: true };
    }
}

// Top-level await (in module context)
export const dynamicImport = await import('./simple_export.js').catch(() => null);
