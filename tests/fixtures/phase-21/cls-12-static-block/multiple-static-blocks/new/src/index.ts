// CLS-12 multiple-static-blocks fixture (new):
// Second static block's assignment (Config.version) removed —
// extractor must find the removal by walking both static blocks.
export class Config {
  static {
    Config.name = 'config';
  }
  static {
    // version removed from the second static block
  }
}
