export class UserRepo {
  #store: Map<string, string> = new Map();
  get(id: string): string | undefined {
    return this.#store.get(id);
  }
}
