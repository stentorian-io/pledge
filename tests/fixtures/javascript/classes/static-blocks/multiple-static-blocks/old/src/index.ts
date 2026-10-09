// CLS-12 multiple-static-blocks fixture (old):
// Class has two static blocks, each with one assignment. The cursor
// loop must walk both blocks.
export class Config {
  static {
    Config.name = 'config';
  }
  static {
    Config.version = '1.0.0';
  }
}
