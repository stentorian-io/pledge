// Utility functions
export function formatDate(date) {
    return date.toISOString();
}
export function parseJSON(str) {
    return JSON.parse(str);
}
export const helpers = {
    trim: (s) => s.trim(),
    upper: (s) => s.toUpperCase(),
};
