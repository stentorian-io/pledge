// Template literal variations

// Basic template literal
export const basic = `Hello, World!`;

// Template with expressions
const name = 'User';
export const withExpression = `Hello, ${name}!`;

// Multi-line template
export const multiLine = `
    This is a
    multi-line
    template literal
`;

// Nested template literals
export const nested = `Outer ${`Inner ${1 + 1}`} Back`;

// Template with function calls
export const withFunction = `Result: ${(() => 'computed')()}`;

// Tagged template literals
export function tag(strings, ...values) {
    return strings.reduce((acc, str, i) =>
        acc + str + (values[i] !== undefined ? `[${values[i]}]` : ''), '');
}

export const tagged = tag`Hello ${name}, you have ${5} messages`;

// Raw strings
export const rawString = String.raw`C:\Users\name\Documents`;

// Template with complex expressions
export const complex = `
    Total: ${[1, 2, 3].reduce((a, b) => a + b, 0)}
    Conditional: ${true ? 'yes' : 'no'}
    Ternary: ${1 > 0 ? 'positive' : 1 < 0 ? 'negative' : 'zero'}
`;

// HTML-like template (common pattern)
export const html = (strings, ...values) => {
    return strings.reduce((result, str, i) => {
        const value = values[i - 1];
        return result + (value !== undefined ? String(value) : '') + str;
    });
};

export const rendered = html`<div class="container">${'content'}</div>`;

// CSS-like template
export const css = `
    .container {
        display: flex;
        padding: ${8 * 2}px;
        margin: ${4}px;
    }
`;
