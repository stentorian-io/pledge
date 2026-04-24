// TYPE-02 fixture (Phase 20 Plan 02): same infer-extends constraint as
// the old/ side, plus one unrelated Helper export so the run is not a
// no-op. Expect 0 `infer-extends-constraint-changed` findings.
export type FirstIfString<T> = T extends { first: infer S extends string } ? S : never;
export type Helper = string;
