// TYPE-02 fixture: infer-extends constraint unchanged
// between versions. Expect 0 `infer-extends-constraint-changed` findings.
export type FirstIfString<T> = T extends { first: infer S extends string } ? S : never;
