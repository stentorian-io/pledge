// CommonJS types module
const Type = {
    STRING: "string",
    NUMBER: "number",
    BOOLEAN: "boolean",
};
function isType(value, type) {
    return typeof value === type;
}
module.exports = { Type, isType };
