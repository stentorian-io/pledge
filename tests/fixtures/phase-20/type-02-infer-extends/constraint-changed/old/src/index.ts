// TYPE-02 fixture (Phase 20 Plan 02): `infer S extends string` on the
// `extends` leg of a conditional type. Uses an object type for the
// constraint envelope because the current type-string parser does not
// handle tuple rest-spread (`...unknown[]`) — see Plan 20-02 SUMMARY.md
// "Deviations" for the fixture-shape note.
export type FirstIfString<T> = T extends { first: infer S extends string } ? S : never;
