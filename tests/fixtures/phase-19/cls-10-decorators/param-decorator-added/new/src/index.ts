interface Service {}
function Inject(_: string) {
  return (_t: any, _k: any, _i: number) => {};
}
export class Foo {
  foo(@Inject('svc') svc: Service) {}
}
