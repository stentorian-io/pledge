// ESM types module
export const Type = {
    STRING: "string",
    NUMBER: "number",
    BOOLEAN: "boolean",
};
export function isType(value, type) {
    return typeof value === type;
}
