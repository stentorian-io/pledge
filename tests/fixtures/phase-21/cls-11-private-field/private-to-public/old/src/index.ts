export class UserRepo {
  #cache: Map<string, string> = new Map();
  get(id: string): string | undefined {
    return this.#cache.get(id);
  }
}
