// TYPE-02 fixture (Phase 20 Plan 02): infer-extends constraint unchanged
// between versions. Expect 0 `infer-extends-constraint-changed` findings.
export type FirstIfString<T> = T extends { first: infer S extends string } ? S : never;
