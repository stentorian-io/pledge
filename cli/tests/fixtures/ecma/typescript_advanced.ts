// Advanced TypeScript features

// Generic types
export function identity<T>(arg: T): T {
    return arg;
}

export class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    getValue(): T {
        return this.value;
    }
}

// Conditional types
export type IsString<T> = T extends string ? true : false;
export type StringOrNumber<T> = T extends string ? string : number;

// Mapped types
export type Readonly2<T> = {
    readonly [P in keyof T]: T[P];
};

export type Optional<T> = {
    [P in keyof T]?: T[P];
};

// Template literal types
export type EventName = `on${Capitalize<string>}`;
export type PropEventHandler<T extends string> = `on${Capitalize<T>}`;

// Infer keyword
export type ReturnTypeOf<T> = T extends (...args: any[]) => infer R ? R : never;
export type ArrayElement<T> = T extends (infer E)[] ? E : never;

// Union and intersection types
export type StringOrNumberOrBoolean = string | number | boolean;
export type Combined = { a: number } & { b: string };

// Type guards
export function isString(value: unknown): value is string {
    return typeof value === 'string';
}

export function isNumber(value: unknown): value is number {
    return typeof value === 'number';
}

// Discriminated unions
export type Success = { type: 'success'; data: unknown };
export type Error2 = { type: 'error'; error: string };
export type Result = Success | Error2;

export function handleResult(result: Result): void {
    switch (result.type) {
        case 'success':
            console.log(result.data);
            break;
        case 'error':
            console.error(result.error);
            break;
    }
}

// Decorators (experimental)
export function logged(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log(`Calling ${key}`);
        return original.apply(this, args);
    };
    return descriptor;
}

// Namespace
export namespace Utils {
    export function helper(): void {}
    export const constant = 42;
}

// Enum
export enum Status {
    Pending = 'PENDING',
    Active = 'ACTIVE',
    Completed = 'COMPLETED'
}

export const enum InlineEnum {
    A = 1,
    B = 2,
    C = 3
}

// Abstract class
export abstract class AbstractService {
    abstract execute(): Promise<void>;

    protected log(message: string): void {
        console.log(message);
    }
}

// Interface with call signature
export interface Callable {
    (arg: string): string;
    property: number;
}

// Interface with index signature
export interface Dictionary<T> {
    [key: string]: T;
}

// Overloaded functions
export function overloaded(x: string): string;
export function overloaded(x: number): number;
export function overloaded(x: string | number): string | number {
    return x;
}

// Async/await with types
export async function asyncTyped(): Promise<string> {
    return 'async result';
}

// As const assertion
export const constArray = [1, 2, 3] as const;
export const constObject = { a: 1, b: 2 } as const;

// Satisfies operator (TS 4.9+)
export const config = {
    port: 3000,
    host: 'localhost'
} satisfies Record<string, string | number>;
