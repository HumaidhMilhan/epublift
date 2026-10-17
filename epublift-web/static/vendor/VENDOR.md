# Vendored: epubveri and epubsana (client-side Validate and Repair)

Two published veripublica WebAssembly packages run in the browser, so the book
never leaves the page: epubveri for Validate, epubsana for Repair. Each is
copied verbatim from npm; only its tiny entry file is replaced by ours.

## epubveri (Validate)

| | |
| --- | --- |
| Package | [`@veripublica/epubveri-wasm`](https://www.npmjs.com/package/@veripublica/epubveri-wasm) **0.29.1** |
| npm integrity | `sha512-P+tGOsgMUS9Q8qw7ehF2u3mcdCcgyG7BGKegM7e+1BdyhIG/1dAuf6kUe+GE+kaPPLgc3e8Wk0NY6x1ZxlkrVw==` |
| Built from | [veripublica/epubveri](https://github.com/veripublica/epubveri) tag `v0.29.1`, commit `a372bc1a51ec6fdcd8d59fd20e4b2856f520a008`, by its `publish-npm.yml` workflow (SLSA provenance attestation on the npm release) |
| Must match | the `epubveri` version in the root `Cargo.toml` — the CLI's `epublift check` and this browser build have to agree about the same book |

### Files

| File | Origin | SHA-256 |
| --- | --- | --- |
| `epubveri_bg.wasm` | verbatim from the package | `046f81f6df070a72e73ff8d84dbf21aa314bc3838981993b0a83cdd4b1d11f80` |
| `epubveri_bg.js` | verbatim from the package (wasm-bindgen glue) | `92adbcc8672c59bd1aae5b31abf5b397d28fe2e6772762f19b79c24d4dabf770` |
| `epubveri-LICENSE`, `epubveri-LICENSE.COMMERCIAL.md` | the package's `LICENSE` and `LICENSE.COMMERCIAL.md` | — |
| `epubveri.js` | **ours** — the browser loader | — |

The package is built for bundlers: its own entry file does
`import * as wasm from "./epubveri_bg.wasm"`, which a browser cannot load
without one. `epubveri.js` replaces only that entry file and does what a bundler
would — instantiate the `.wasm` with the glue as its import module, hand the
instance to the glue, run the start function. Nothing published is modified.

### Updating

Bump the `epubveri` crate in `Cargo.toml` in the same change, then:

```sh
npm pack @veripublica/epubveri-wasm@<version>      # prints the integrity
npm view @veripublica/epubveri-wasm@<version> gitHead
tar xzf veripublica-epubveri-wasm-<version>.tgz
cp package/epubveri_bg.wasm package/epubveri_bg.js epublift-web/static/vendor/
cp package/LICENSE epublift-web/static/vendor/epubveri-LICENSE
cp package/LICENSE.COMMERCIAL.md epublift-web/static/vendor/epubveri-LICENSE.COMMERCIAL.md
shasum -a 256 epublift-web/static/vendor/epubveri_bg.*
```

Update the table above, check that the package still imports only from
`./epubveri_bg.js` and still exports `validate`, `version` and
`__wbindgen_start` (the loader relies on those three), and check that the
`Report` shape in `package/epubveri.d.ts` still matches what `app.js` reads.

## epubsana (Repair)

| | |
| --- | --- |
| Package | [`@veripublica/epubsana-wasm`](https://www.npmjs.com/package/@veripublica/epubsana-wasm) **0.31.0** |
| npm integrity | `sha512-5EuLcgsUFDXockXe0mXbz+hO3SBvasGu7NWlBJZPKF0klzo2Es5vy0gnKC2iKKjuAHzaavkKRfFXLBpro+XRgg==` |
| Built from | [veripublica/epubsana](https://github.com/veripublica/epubsana) tag `v0.31.0`, commit `518341e30eaa1a0e224079aad55c8b8687d7d3f9` |
| Must match | the `epubsana` version in the root `Cargo.toml` — `epublift repair` and this browser build run the same `repair()`, so the same book with the same fixes approved comes back identical |

It carries its own copy of epubveri (the repairer validates before and after
every repair), so it does not share the Validate module above. The two must
still be on the same epubveri minor; epubsana's `Cargo.toml` states its floor.

### Files

| File | Origin | SHA-256 |
| --- | --- | --- |
| `epubsana_wasm_bg.wasm` | verbatim from the package | `66b2b2138b6098bcffb1fb6df00b5b1f24c83d3db1af287e089bf9dd6eceefc2` |
| `epubsana_wasm_bg.js` | verbatim from the package (wasm-bindgen glue) | `7bdb6ec6f02adfacf0203aff11714e21428a6c0c0958c90b5ac2e8b8f3019306` |
| `epubsana-LICENSE`, `epubsana-LICENSE-COMMERCIAL.md` | the package's `LICENSE` and `LICENSE-COMMERCIAL.md` | — |
| `epubsana.js` | **ours** — the browser loader, the same shape as `epubveri.js` | — |

### Updating

Bump the `epubsana` crate in `Cargo.toml` in the same change, then:

```sh
npm pack @veripublica/epubsana-wasm@<version>      # prints the integrity
npm view @veripublica/epubsana-wasm@<version> gitHead
tar xzf veripublica-epubsana-wasm-<version>.tgz
cp package/epubsana_wasm_bg.wasm package/epubsana_wasm_bg.js epublift-web/static/vendor/
cp package/LICENSE epublift-web/static/vendor/epubsana-LICENSE
cp package/LICENSE-COMMERCIAL.md epublift-web/static/vendor/epubsana-LICENSE-COMMERCIAL.md
shasum -a 256 epublift-web/static/vendor/epubsana_wasm_bg.*
```

Update the table above, check that the package still imports only from
`./epubsana_wasm_bg.js` and still exports `Session`, `version`,
`__wbg_set_wasm` and `__wbindgen_start`, and check that the `Session`, `Plan`,
`Fix` and `Report` shapes in `package/epubsana_wasm.d.ts` still match what
`app.js` reads. Note the two indices: `Fix.index` (0-based) is what
`Session.repair` takes; a report item's `data.index` is 1-based, for the CLI.
