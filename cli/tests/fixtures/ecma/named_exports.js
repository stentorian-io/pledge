// Module with only named exports (no default)
export const CONFIG = {
    debug: false,
    version: "2.0.0",
};

export function initialize() {
    console.log("Initialized");
}

export function shutdown() {
    console.log("Shutdown");
}

export const utils = {
    formatDate: (date) => date.toISOString(),
    parseDate: (str) => new Date(str),
};
