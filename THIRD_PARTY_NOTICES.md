# Third-party notices

kiki-agent vendors and adapts the following MIT-licensed projects:

- **kage** (https://github.com/QaidVoid/kage, MIT): desktop visual layer
  (`theme.rs`, `fonts.rs`, `themes.rs`, `icons.rs`, `shapes.rs`, `ui/forms.rs`,
  `ui/shell.rs`, `ui/transcript/{prose,markdown,list}.rs`), bundled fonts and
  Lucide icons. Icon artwork is ISC (Lucide); fonts are SIL OFL 1.1
  (Inter, JetBrains Mono, Schibsted Grotesk).
- **pi-rust** (https://github.com/bigfish1913/pi-rust, MIT): the vendored
  `vendor/pi-rust/crates/rpi-*` engine crates (sources copied unmodified).
- **rpi-package** (https://github.com/pi-rust/rpi-package, MIT):
  `crates/pure-extensions-packages/pure-codemode` is ported from
  `rpi-codemode` 0.1.15 (schema keywords adapted to `x-pure-*`);
  `crates/pure-extensions-packages/pure-permission-gate` is ported from
  `rpi-permission-gate` 0.1.4 with the local `permissions.json` rule and
  `confine_paths` layer. The former depends on the MIT-licensed QuickJS
  bindings `rquickjs`.
- **oh-my-pi** (https://github.com/can1357/oh-my-pi, MIT):
  `crates/pure-engine/pure-tools/src/walker/` is ported from `pi-walker`
  (`walk`, `ignore`, `pattern`, `glob`; the fs layer is re-targeted onto
  `ExecutionEnv` and the provider/PCRE2 support is dropped), and
  `crates/pure-engine/pure-tools/src/tools/edit_fuzzy/` is ported from
  `pi-edit` (`fuzzy.rs`, `text.rs`; `xutf` is replaced by
  `unicode-normalization` for NFC).
- **rtk** (https://github.com/rtk-ai/rtk): the prebuilt `rtk` 1.0.1 binaries
  under `crates/pure-extensions-packages/pure-rtk/vendor/rtk/` are redistributed
  unmodified and invoked by the `pure-rtk` extension. The CLI is Apache-2.0;
  this build also contains the BUSL-1.1 `rtk-engine` (its LICENSE files ship in
  the source tree at `reference/rtk-develop/`). Ship a `--no-default-features`
  build in its place if a pure Apache-2.0 artifact is required — `rtk pipe` and
  `rtk rewrite` are unaffected by that feature.

The `reference/` directory holds the original checkouts used for the port and is
not part of the build.
