// Deeply nested structures to test parser limits

export const deepObject = {
    level1: {
        level2: {
            level3: {
                level4: {
                    level5: {
                        level6: {
                            level7: {
                                level8: {
                                    level9: {
                                        level10: {
                                            value: 'deep'
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
};

export const deepArray = [[[[[[[[[[['nested']]]]]]]]]];

export function deeplyNestedFunction() {
    return function() {
        return function() {
            return function() {
                return function() {
                    return function() {
                        return 'deep';
                    };
                };
            };
        };
    };
}

export const deepCallback = (cb) => {
    return cb(() => {
        return (cb2) => {
            return cb2(() => {
                return (cb3) => {
                    return cb3(() => {
                        return 'done';
                    });
                };
            });
        };
    });
};

export const deepTernary = true
    ? true
        ? true
            ? true
                ? true
                    ? 'very true'
                    : 'false'
                : 'false'
            : 'false'
        : 'false'
    : 'false';
