// Mixed exports - various declaration types
export const VERSION = '2.0.0';

export let mutableValue = 42;

export var legacyVar = 'legacy';

export function syncFunction() {
    return 'sync';
}

export async function asyncFunction() {
    return Promise.resolve('async');
}

export class ServiceClass {
    method() {}
}

export default class DefaultService {
    execute() {}
}
