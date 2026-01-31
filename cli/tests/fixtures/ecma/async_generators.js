// Async and generator functions

// Basic async function
export async function fetchData() {
    const response = await fetch('/api/data');
    return response.json();
}

// Async arrow function
export const asyncArrow = async () => {
    return await Promise.resolve('done');
};

// Generator function
export function* numberGenerator() {
    yield 1;
    yield 2;
    yield 3;
}

// Async generator function
export async function* asyncGenerator() {
    yield await Promise.resolve(1);
    yield await Promise.resolve(2);
    yield await Promise.resolve(3);
}

// Generator with delegation
export function* delegatingGenerator() {
    yield* [1, 2, 3];
    yield* numberGenerator();
}

// Async iterator
export const asyncIterable = {
    async *[Symbol.asyncIterator]() {
        yield 'a';
        yield 'b';
        yield 'c';
    }
};

// Complex async/await patterns
export async function complexAsync() {
    const [a, b, c] = await Promise.all([
        Promise.resolve(1),
        Promise.resolve(2),
        Promise.resolve(3)
    ]);

    try {
        await Promise.reject(new Error('test'));
    } catch (e) {
        // Handled
    } finally {
        console.log('done');
    }

    return { a, b, c };
}
