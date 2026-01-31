// Default export via expression
function processData(data) {
    return data.map(x => x * 2);
}

const config = {
    debug: true,
    version: '1.0.0'
};

export default processData;
export { config };
