# Changelog

## [0.4.0](https://github.com/stentorian-io/pledge/compare/v0.3.0...v0.4.0) (2026-11-09)


### ⚠ BREAKING CHANGES

* omitted action inputs now select root and workspace packages with same-major baselines. Set path: . and against: latest to retain the prior package and baseline selection. The action adds advisory status for complete 0.x breaking comparisons; standalone CLI behavior is unchanged.
* the default cjs-named-export-removed check reports Major when an existing named ESM import disappears even if require() still works. Review new findings for CommonJS rewrites; missing Node parser evidence makes the comparison incomplete rather than compatible.
* default checks now include packed dotfiles, libc requirements and CSS @property registrations. Review newly reported breaking findings in existing CI inputs. Rust callers must use an insertion-ordered IndexMap for TsconfigPaths.paths; resolve_tsconfig_alias now returns mapped targets, and callers that need file resolution must apply ecosystem extension probing.
* default analysis adds Major function-constructor-removed, export-writability-removed, export-enumerability-removed and node-syntax-requirement-raised findings. Report consumers should recognize these rule IDs. Packages removing these capabilities must preserve them or release a breaking version; newly introduced resource-management syntax must align engines.node with supported runtimes.
* the acquire::registry Rust module and its NpmClient, PackageMetadata, fetch_package_metadata and download_tarball exports are removed. Library callers should acquire packages with npm and use acquire::extract_tgz or acquire::extract_tgz_from_bytes for extraction.
* the executable, Rust crate, and managed cache directory are now named pledge.
* Rust library imports now live under app, core, languages, and ecosystems. Node dependency preparation returns DependencyError; compiler and CLI callers retain their existing error reporting through conversion.

### Features

* add a GitHub Action for package compatibility checks ([8217d0d](https://github.com/stentorian-io/pledge/commit/8217d0db0fd77eed4c81282edfee9b73cc84c122))
* automatically check workspace packages against their major ([77af523](https://github.com/stentorian-io/pledge/commit/77af523ae124fbb4a220ac0204b22ba421b72639))
* support npm package checks in CI ([53ce903](https://github.com/stentorian-io/pledge/commit/53ce90358c34de0319d57c6322ab30dd63e5b0a2))


### Bug Fixes

* account for CommonJS export replacement and mutation ([c761807](https://github.com/stentorian-io/pledge/commit/c761807e0cbcf45346f33b2ddbc2a2cd441920f9))
* allow calls to omit added rest parameters ([ea83f52](https://github.com/stentorian-io/pledge/commit/ea83f520b9c9f3b536c69d24e66c30d5039b047e))
* bound package analysis and preserve declaration evidence ([271ec08](https://github.com/stentorian-io/pledge/commit/271ec086760b96e819aef5fe05547e213888d86c))
* bound package scans and report unreadable paths ([afb5999](https://github.com/stentorian-io/pledge/commit/afb59996cfbfe0413c10f2ec99be55c93ee3d8e6))
* bound stylesheet reads and handle deeply nested rules ([dbb5ff2](https://github.com/stentorian-io/pledge/commit/dbb5ff2d73c2777191f509bd98e3f6c184d18965))
* bound total package scan work ([dc2235d](https://github.com/stentorian-io/pledge/commit/dc2235d462fe6126763ba24ed25dfd8d6939d7ff))
* bound type parser recursion and work ([ffc77b4](https://github.com/stentorian-io/pledge/commit/ffc77b4c0f2565f51d35694e59737df8731ac6e6))
* classify package maps by their public API effects ([54bfd7a](https://github.com/stentorian-io/pledge/commit/54bfd7a8a39c8dc9f8646d86a9bf404fc8478e40))
* compare APIs selected by added export conditions ([19f3fcd](https://github.com/stentorian-io/pledge/commit/19f3fcddb84ca1bb7d833a3517b8dd56665798ff))
* compare class construction and static initialization contracts ([4544e58](https://github.com/stentorian-io/pledge/commit/4544e58bb9a82fee040802bd3a923e641fdd2997))
* compare class contracts independently of implements clauses ([c1d968d](https://github.com/stentorian-io/pledge/commit/c1d968d80f5774b15ed6f95b35ddc65f5a4d88a9))
* compare complete secondary overload contracts ([b1c343f](https://github.com/stentorian-io/pledge/commit/b1c343fae55e0ae3ce7a13f5739fbaaefe030fb9))
* compare JSON Schema required property sets ([d88c75e](https://github.com/stentorian-io/pledge/commit/d88c75e7a086d33bb0f411021b90c7b916ce848e))
* compare selected stylesheet APIs across path changes ([743a2fd](https://github.com/stentorian-io/pledge/commit/743a2fdcad73c5c3843d2a7cb97fe8fbb5bfbcf6))
* compare tuple requirements independently of labels ([0dfc975](https://github.com/stentorian-io/pledge/commit/0dfc975d3372fc022a0718befe3bfc80b79f6753))
* compare types reachable through public declarations ([a9a6d43](https://github.com/stentorian-io/pledge/commit/a9a6d4311bee16709168d74df39f45d31b231ede))
* detect modules that lose synchronous loading support ([a966eaa](https://github.com/stentorian-io/pledge/commit/a966eaaa09b5bd86b59d6cc725e4789395b6c21c))
* detect narrowed package version and platform support ([4ab5053](https://github.com/stentorian-io/pledge/commit/4ab50534bf6d1744d08cfacf24a37400b9d2921c))
* detect removed CommonJS names exposed to ESM imports ([814a845](https://github.com/stentorian-io/pledge/commit/814a845414ce94454fcbd60d392856dba9e2be1d))
* distinguish reflective surfaces from opaque dispatch ([b602586](https://github.com/stentorian-io/pledge/commit/b60258672dabbf49f06b33a907eb1fdbc40de30f))
* evaluate public contracts with the typescript compiler ([206b3b3](https://github.com/stentorian-io/pledge/commit/206b3b3b0c1f5d561b979450e989f703f69f9430))
* evaluate public types without a language server ([940345c](https://github.com/stentorian-io/pledge/commit/940345c4affb151eaa0be1658bd1fcdb59427348))
* exclude non-public exports from structural comparison ([8a4d5de](https://github.com/stentorian-io/pledge/commit/8a4d5de368170c60be185e6f26d88ec494c89125))
* follow conditional commonjs and generated package exports ([809f70a](https://github.com/stentorian-io/pledge/commit/809f70ad2180668b9b45cfebfc60bfe830b02f8a))
* honor package resolution and distributed asset contracts ([2a89d25](https://github.com/stentorian-io/pledge/commit/2a89d252ced4df996707f486184d4afad19c3dc0))
* install and select a complete language server toolchain ([4dafdb5](https://github.com/stentorian-io/pledge/commit/4dafdb5eac7c7443169ec21a40bdf66d7518b523))
* isolate and bound language server dependency setup ([555e4c1](https://github.com/stentorian-io/pledge/commit/555e4c1d03737075228f28d9e833fc44f7a199bf))
* isolate runtime dependency resolution from development peers ([a8fe977](https://github.com/stentorian-io/pledge/commit/a8fe9777a295b0130456679ea9855bc01cade90c))
* pair reference directive preserve changes ([fdc2708](https://github.com/stentorian-io/pledge/commit/fdc2708f5d007a5a9d4668cb629b4d6def8e6b5c))
* prepare dependencies before resolving package exports ([43f916b](https://github.com/stentorian-io/pledge/commit/43f916b995c5df45ac091b7404f762e0a41d7e0d))
* preserve accessor write types and subclass contracts ([3681af8](https://github.com/stentorian-io/pledge/commit/3681af819acbb348485d3e5838ce444767500701))
* preserve callable receivers and narrowing guarantees ([97d6b33](https://github.com/stentorian-io/pledge/commit/97d6b33cd768bc9961005ba0cfa8b1c45114a2f4))
* preserve conditional export array fallbacks ([9d46643](https://github.com/stentorian-io/pledge/commit/9d4664308090f737035964bf27d35e60298924cb))
* preserve exact optional property contracts ([a062556](https://github.com/stentorian-io/pledge/commit/a0625564951807cdddbb061198d54f3181ca5653))
* preserve export identities and callable contracts ([e0c6651](https://github.com/stentorian-io/pledge/commit/e0c665191c6da426a457ec4820e9e8dbee590cb3))
* preserve getter and setter identity when reordered ([9a80f65](https://github.com/stentorian-io/pledge/commit/9a80f65b0a81af19683ba944f0d46270c3fde103))
* preserve implicit class constructors ([65fd9c8](https://github.com/stentorian-io/pledge/commit/65fd9c81bf9ead4ed6a8f02e138cd1213a7f6a7a))
* preserve independent stylesheet evidence ([636a78c](https://github.com/stentorian-io/pledge/commit/636a78ccb3aa7aaa847f7f245dab003aec9a18b8))
* preserve initialized exports and TypeScript consumer contracts ([b2a4287](https://github.com/stentorian-io/pledge/commit/b2a42870ae844e69916b8565bef239e85eff9c9f))
* preserve iterator next input variance ([dadc8c3](https://github.com/stentorian-io/pledge/commit/dadc8c38efd394e94645d0a618f6144879e71e13))
* preserve JavaScript and TypeScript package contracts ([b1dad9d](https://github.com/stentorian-io/pledge/commit/b1dad9d563221f22b1c1d44355ccbc4ce008ef2e))
* preserve prerelease support when comparing version ranges ([294c220](https://github.com/stentorian-io/pledge/commit/294c220eef8de7b08c1ab7a4deb2b60dcd1402ab))
* preserve public entry points and resolution contexts ([77abdd8](https://github.com/stentorian-io/pledge/commit/77abdd830cce4ec2f79ff6af9666e458c286a528))
* preserve quoted paths in JSONC configuration ([6f33f8a](https://github.com/stentorian-io/pledge/commit/6f33f8a7a19b9f6f624dbb292f5a93ab5d9b9b71))
* preserve symbolic types and generic parameter identity ([e715a3d](https://github.com/stentorian-io/pledge/commit/e715a3dc6f69785b9995bd395a2ae34d284d90c8))
* preserve writes through returned properties ([67c6b64](https://github.com/stentorian-io/pledge/commit/67c6b6498c27a4b76eda103f8e67a41de44916f3))
* recognize omittable constructor arguments ([87f3409](https://github.com/stentorian-io/pledge/commit/87f3409533541fcf87401b61357df4c1e80cceae))
* reject invalid package export targets ([68ebe08](https://github.com/stentorian-io/pledge/commit/68ebe089d9678ca36ac8481d693286271b8bd211))
* report incomplete discovery and type analysis ([237d68b](https://github.com/stentorian-io/pledge/commit/237d68bef6b654493c5f7ad9a1c94048cc097c82))
* report incomplete JSON asset analysis ([546ad6a](https://github.com/stentorian-io/pledge/commit/546ad6a5fb6f9fb16a84def33c8341fe2cb626d9))
* resolve computed property and symbol export keys ([2387815](https://github.com/stentorian-io/pledge/commit/238781548228b797fadb27f20385884fcfe8b8be))
* resolve modern package types and unicode source positions ([fda02ef](https://github.com/stentorian-io/pledge/commit/fda02efa6350caaaff7aa4a24d25c941f1b1ad02))
* resolve re-export bindings and star export semantics ([a6f748c](https://github.com/stentorian-io/pledge/commit/a6f748ce4b45829d4997da3c3d3f1c23f4b3eaaa))
* resolve TypeScript export assignment contracts ([8dcd261](https://github.com/stentorian-io/pledge/commit/8dcd261d0207d2e212b0bf641f48ed6cb87dc4aa))
* retain accurate JavaScript and npm interface evidence ([5ee59ff](https://github.com/stentorian-io/pledge/commit/5ee59ff91a8254a285e23fbefc28314a9cbf28c1))
* retain generic constraints and readonly contracts ([d41e3d9](https://github.com/stentorian-io/pledge/commit/d41e3d9c62be00414e2390eecb6cb74a4e71bec1))
* retain symbol identities and annotated export types ([f5ba630](https://github.com/stentorian-io/pledge/commit/f5ba6305ef0121627532fca69e9c65a6c2de978a))
* retain uncertainty in CommonJS alias mutations ([476b391](https://github.com/stentorian-io/pledge/commit/476b3918284b222f41eb34277f74e9ec551cf9ba))


### Performance

* reuse compiler processes across isolated evaluations ([8b63045](https://github.com/stentorian-io/pledge/commit/8b6304576cc8347c62fc481bd92a91b3b31ccce1))
* reuse source analysis across package entry points ([ee91a14](https://github.com/stentorian-io/pledge/commit/ee91a143f3786927df4271d3e80640e88a7033d7))


### Internal

* organize source by language and ecosystem ([ceabc3e](https://github.com/stentorian-io/pledge/commit/ceabc3e58a3c74f5345deb3790732f6286a35c8b))
* remove the unused direct registry client ([ad88517](https://github.com/stentorian-io/pledge/commit/ad88517c541698ebb8e3d3bd9d951da05d3b5707))
* rename the CLI and crate to pledge ([e4e334c](https://github.com/stentorian-io/pledge/commit/e4e334cc7ed28ed839a2ae404f882cf77c702deb))

## Changelog

Release Please maintains entries for automated releases here.

Earlier development milestones retain their original tags:
[v0.1](https://github.com/stentorian-io/pledge/releases/tag/v0.1),
[v0.2](https://github.com/stentorian-io/pledge/releases/tag/v0.2), and
[v0.3](https://github.com/stentorian-io/pledge/releases/tag/v0.3).
The automation baseline is 0.3.0 at the existing v0.3 commit; no historical
tag or published artifact is rewritten.
