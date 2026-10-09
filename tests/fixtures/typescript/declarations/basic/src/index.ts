export const VERSION: string = '1.0.0';

export interface Config {
  debug: boolean;
}

export function configure(opts: Config): void {}

export default class App {}
