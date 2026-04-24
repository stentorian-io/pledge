// TYPE-02 fixture (Phase 20 Plan 02): constraint changed from `string`
// to `number` — should fire exactly 1 Major
// `infer-extends-constraint-changed` finding.
export type FirstIfString<T> = T extends { first: infer S extends number } ? S : never;
