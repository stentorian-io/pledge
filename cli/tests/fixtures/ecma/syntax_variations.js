// Various syntax edge cases

// Trailing commas
export const arrayTrailing = [1, 2, 3,];
export const objectTrailing = { a: 1, b: 2, };
export function funcTrailing(a, b, c,) { return a + b + c; }

// No semicolons (ASI)
export const noSemi1 = 1
export const noSemi2 = 2
export function noSemiFunc() {
    return 'value'
}

// Multiple statements per line
export const multi1 = 1; export const multi2 = 2; export const multi3 = 3;

// Spread in various contexts
export const spreadArray = [...[1, 2], ...[3, 4]];
export const spreadObject = { ...{ a: 1 }, ...{ b: 2 } };
export const spreadCall = Math.max(...[1, 2, 3]);

// Rest parameters
export function restParams(first, ...rest) {
    return [first, rest];
}

// Default parameters
export function defaultParams(a = 1, b = 'default', c = []) {
    return { a, b, c };
}

// Computed properties
const key1 = 'computed';
const key2 = 'another';
export const computedProps = {
    [key1]: 'value1',
    [`${key2}_key`]: 'value2',
    [1 + 2]: 'value3'
};

// Shorthand properties
const x = 1, y = 2;
export const shorthand = { x, y };

// Method shorthand
export const methods = {
    regular() { return 'regular'; },
    async asyncMethod() { return 'async'; },
    *generator() { yield 1; }
};

// Arrow function variations
export const arrow1 = () => 'no params';
export const arrow2 = x => x * 2;
export const arrow3 = (x, y) => x + y;
export const arrow4 = (x) => { return x; };
export const arrow5 = () => ({});  // Return object literal

// Immediately invoked function expressions
export const iife = (function() {
    return 'iife';
})();

export const iifeArrow = (() => {
    return 'arrow iife';
})();

// Comma operator
export const comma = (1, 2, 3);

// Void operator
export const voidOp = void 0;

// typeof
export const typeofCheck = typeof undefined;

// instanceof
export const instanceCheck = [] instanceof Array;

// in operator
export const inCheck = 'toString' in {};

// delete operator
const obj = { a: 1 };
delete obj.a;
export { obj as deletedFrom };

// Sequence expressions in various contexts
export const seq = (console.log('a'), console.log('b'), 'result');

// Labels (rarely used but valid)
outer: for (let i = 0; i < 3; i++) {
    inner: for (let j = 0; j < 3; j++) {
        if (i === j) continue outer;
    }
}

// Empty statement
;

// Empty block
{}

export const final = 'end';
