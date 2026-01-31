// Advanced class features

// Basic class
export class BasicClass {
    constructor(name) {
        this.name = name;
    }

    greet() {
        return `Hello, ${this.name}`;
    }
}

// Class with inheritance
export class DerivedClass extends BasicClass {
    constructor(name, title) {
        super(name);
        this.title = title;
    }

    greet() {
        return `${this.title} ${super.greet()}`;
    }
}

// Class with static members
export class StaticMembers {
    static staticProp = 'static value';
    static #privateStatic = 'private static';

    static staticMethod() {
        return this.staticProp;
    }

    static {
        console.log('Static initialization');
    }
}

// Class with private members
export class PrivateMembers {
    #privateField = 42;
    #privateMethod() {
        return this.#privateField;
    }

    getPrivate() {
        return this.#privateMethod();
    }
}

// Class with getters and setters
export class GetterSetter {
    #value = 0;

    get value() {
        return this.#value;
    }

    set value(newValue) {
        this.#value = newValue;
    }
}

// Abstract-like class (ES pattern)
export class AbstractBase {
    constructor() {
        if (new.target === AbstractBase) {
            throw new Error('Cannot instantiate abstract class');
        }
    }

    abstractMethod() {
        throw new Error('Must be implemented');
    }
}

// Class expression
export const ClassExpression = class NamedClassExpr {
    method() {
        return 'class expression';
    }
};

// Computed method names
const methodName = 'dynamicMethod';
export class ComputedMethods {
    [methodName]() {
        return 'dynamic';
    }

    ['another' + 'Method']() {
        return 'another';
    }
}

// Mixin pattern
const Mixin = (Base) => class extends Base {
    mixinMethod() {
        return 'mixin';
    }
};

export class MixedClass extends Mixin(BasicClass) {
    ownMethod() {
        return 'own';
    }
}
