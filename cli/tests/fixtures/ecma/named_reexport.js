// Named re-exports
const internalHelper = () => 'internal';
const anotherHelper = () => 'another';

export { internalHelper as helper, anotherHelper };
