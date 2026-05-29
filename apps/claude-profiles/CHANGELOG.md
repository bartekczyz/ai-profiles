# Changelog

## [0.5.0](https://github.com/bartekczyz/claude-profiles/compare/v0.4.0...v0.5.0) (2026-05-29)


### Added

* default profile row + usage-card polish ([#21](https://github.com/bartekczyz/claude-profiles/issues/21)) ([7df9dc4](https://github.com/bartekczyz/claude-profiles/commit/7df9dc4242cdafc747b046a28e6a495f56dcf439))

## [0.4.0](https://github.com/bartekczyz/claude-profiles/compare/v0.3.0...v0.4.0) (2026-05-28)


### Added

* **usage:** add daily segments and on-pace marker to weekly meters ([0f74bb7](https://github.com/bartekczyz/claude-profiles/commit/0f74bb7c53c325ba25973998dcfa4ac7ed43f939))
* **usage:** auto-trigger Claude Code refresh on unauthorized quota fetch ([b87dbeb](https://github.com/bartekczyz/claude-profiles/commit/b87dbeb9477e3428b41b3945a6e52bc64ce56e07))
* **usage:** compose command and register it ([0b5dbdf](https://github.com/bartekczyz/claude-profiles/commit/0b5dbdf6e31e7688c551605eaa7c0ade3e9d34f2))
* **usage:** defensive credentials reader ([16a4f6a](https://github.com/bartekczyz/claude-profiles/commit/16a4f6a8d767cf9985a2cf311df5e2f2892396df))
* **usage:** frontend types, hook and defensive narrowing ([555a0fc](https://github.com/bartekczyz/claude-profiles/commit/555a0fcfebf3890e534dbeb099aee754588982ad))
* **usage:** per-profile Claude usage stats ([5eb69fd](https://github.com/bartekczyz/claude-profiles/commit/5eb69fd832ba034f66f7d0f6586eb2b704b2d5ab))
* **usage:** per-profile usage card on profile detail ([07c681d](https://github.com/bartekczyz/claude-profiles/commit/07c681d0800cf827633b6ea7c3084fb41893e3d6))
* **usage:** polish card with countdown, equal-width bars and responsive labels ([da71b9c](https://github.com/bartekczyz/claude-profiles/commit/da71b9c7fbe3ccc5604ee997481f4823aa3a8dc7))
* **usage:** quota http client with defensive parsing ([1923fe1](https://github.com/bartekczyz/claude-profiles/commit/1923fe19ee7ff073cbc1b0abf82f9209be671341))
* **usage:** scaffold rust module + price table ([478c142](https://github.com/bartekczyz/claude-profiles/commit/478c142e860ebddfdaab59d3814b93379b6a2d11))


### Fixed

* **usage:** fall back to macos keychain for credentials ([79b5a5c](https://github.com/bartekczyz/claude-profiles/commit/79b5a5cca9e14fbe330179792b0df7dce33afd8e))
* **usage:** gate macOS-only credential lookups behind cfg(target_os = "macos") ([237444f](https://github.com/bartekczyz/claude-profiles/commit/237444fbde1705c6eadd13ae1795e01a48935854))
* **usage:** harden quota fetch — explicit 4xx mapping, body size pre-check, token trim ([b60a0ad](https://github.com/bartekczyz/claude-profiles/commit/b60a0ad5e9941a8dcbf35d54d4a5fc61dae06814))
* **usage:** isolate usage cache from profile invalidations, reset error boundary across profiles ([85963e4](https://github.com/bartekczyz/claude-profiles/commit/85963e43a34aea30cfef905d07acd5071d1008d4))
* **usage:** keep local breakdown visible when quota fetch fails, refine unauthorized copy ([3c46af8](https://github.com/bartekczyz/claude-profiles/commit/3c46af8c67a0215ce9e400b4523ad5396dcb0a29))
* **usage:** serialise + back off CLI token refresh, wait for token write ([09aae53](https://github.com/bartekczyz/claude-profiles/commit/09aae536f2e4c23ad3c3b653f3266ec2c2f57157))
* **usage:** show explicit message for every quota error state ([b11a514](https://github.com/bartekczyz/claude-profiles/commit/b11a51435cedd3bb696b3ba29b1241564a011775))
* **usage:** split 429 rate limit out from generic network error ([0884ec9](https://github.com/bartekczyz/claude-profiles/commit/0884ec9dea4989081baa425bdd6f01b26d1716c0))
* **usage:** treat utilization as 0-100 percentage, not 0-1 fraction ([ec83b21](https://github.com/bartekczyz/claude-profiles/commit/ec83b2141af9609a93f92f349dbaccbee70807db))


### Changed

* **profiles:** rename useProfileUsage to useProfileLastUsed ([52aa7b5](https://github.com/bartekczyz/claude-profiles/commit/52aa7b5c55496217075fb8be1f10dae707dd6865))

## [0.3.0](https://github.com/bartekczyz/claude-profiles/compare/v0.2.2...v0.3.0) (2026-05-25)


### Added

* **empty-state:** guide users through Claude install when neither surface is detected ([b46aff6](https://github.com/bartekczyz/claude-profiles/commit/b46aff6e79f32e9e548d1ee542f2b2ad4f35b6b1))
* **empty-state:** guide users through Claude install when neither surface is detected ([2e922e1](https://github.com/bartekczyz/claude-profiles/commit/2e922e14b348b243fcf60f0d1f7d84ac1c2adec2))
* **settings:** show the app version next to the Updates row ([aa1054d](https://github.com/bartekczyz/claude-profiles/commit/aa1054d7118512fa663768b34a32cef5cc3b472f))


### Fixed

* **about:** correct version display and tidy About / Settings ([6bf20fe](https://github.com/bartekczyz/claude-profiles/commit/6bf20fe07a70049ac74129c6f3d6748d6ead7d30))
* **about:** use a clearer dialog title and drop the long subtitle ([ef4dd0d](https://github.com/bartekczyz/claude-profiles/commit/ef4dd0d93a126b36274900f048db9627dcfa890d))

## [0.2.2](https://github.com/bartekczyz/claude-profiles/compare/v0.2.1...v0.2.2) (2026-05-25)


### Fixed

* **updater:** relaunch the app after downloadAndInstall ([689b6c1](https://github.com/bartekczyz/claude-profiles/commit/689b6c18e74b03999e63a201632f3012632a798b))
* **updater:** relaunch the app after downloadAndInstall ([7bfd98f](https://github.com/bartekczyz/claude-profiles/commit/7bfd98fbd5a3faf0cde4d3c170a3864db66e2c4d))

## [0.2.1](https://github.com/bartekczyz/claude-profiles/compare/v0.2.0...v0.2.1) (2026-05-25)


### Fixed

* **boot:** paint brand background + loader before the JS bundle runs ([0d32ff3](https://github.com/bartekczyz/claude-profiles/commit/0d32ff30701a2c25066c24938433669aef25d173))


### Performance

* **deps:** cache shell-PATH lookup and short-circuit via process env ([7b2135e](https://github.com/bartekczyz/claude-profiles/commit/7b2135e18c9b254fe35072fac690e8b92dfe6f4e))
* faster, less ugly app startup ([19225da](https://github.com/bartekczyz/claude-profiles/commit/19225da82f6fe6443f0afee4d12467378ed9cd89))
* **migration:** defer the directory-size walks off the boot path ([718fe9e](https://github.com/bartekczyz/claude-profiles/commit/718fe9efb0b8fb4fc70b1074df29ba2d014cfa98))

## [0.2.0](https://github.com/bartekczyz/claude-profiles/compare/v0.1.1...v0.2.0) (2026-05-25)


### Added

* **migration:** redesign import dialog with side-by-side layout ([5cc2d25](https://github.com/bartekczyz/claude-profiles/commit/5cc2d256e5732522aed0aeec966dfe148d59f4c6))
* onboarding & migration polish ([0314d9b](https://github.com/bartekczyz/claude-profiles/commit/0314d9b38ef26c3704464c10b0f07c5145267c23))
* **onboarding:** replace auto-migration prompt with a fork dialog ([c2b6380](https://github.com/bartekczyz/claude-profiles/commit/c2b638062ff1c9418fbe71d6a1c24797832e5273))


### Fixed

* **onboarding:** restyle welcome dialog with the Atelier primitive ([8c8b110](https://github.com/bartekczyz/claude-profiles/commit/8c8b110c784a069e69a7f644b54707365116c567))
* **settings:** keep settings open when re-import is dismissed ([dea9c33](https://github.com/bartekczyz/claude-profiles/commit/dea9c33b97f6aa97103e91111b9cabafbce1575d))
* **ui:** polish onboarding controls ([1154350](https://github.com/bartekczyz/claude-profiles/commit/1154350296b57c0f355b0524555320621a8f7648))
* **updater:** stop overriding the platform key to darwin-universal ([cf5339b](https://github.com/bartekczyz/claude-profiles/commit/cf5339bcb6780aea875f6ba05b6ec97881b4ede2))

## [0.1.1](https://github.com/bartekczyz/claude-profiles/compare/v0.2.0...v0.1.1) (2026-05-24)


* release 0.1.1 ([8dfbb73](https://github.com/bartekczyz/claude-profiles/commit/8dfbb73423d39595a644d6a4deaf4a9c68cb7287))


### Added

* **app:** focus sidebar search with ⌘F ([e74d248](https://github.com/bartekczyz/claude-profiles/commit/e74d2489fea67c327344b58de324a2e15a22bd8c))
* **migration:** redesign import dialog with side-by-side layout ([5cc2d25](https://github.com/bartekczyz/claude-profiles/commit/5cc2d256e5732522aed0aeec966dfe148d59f4c6))
* onboarding & migration polish ([0314d9b](https://github.com/bartekczyz/claude-profiles/commit/0314d9b38ef26c3704464c10b0f07c5145267c23))
* **onboarding:** replace auto-migration prompt with a fork dialog ([c2b6380](https://github.com/bartekczyz/claude-profiles/commit/c2b638062ff1c9418fbe71d6a1c24797832e5273))


### Fixed

* **onboarding:** restyle welcome dialog with the Atelier primitive ([8c8b110](https://github.com/bartekczyz/claude-profiles/commit/8c8b110c784a069e69a7f644b54707365116c567))
* **settings:** keep settings open when re-import is dismissed ([dea9c33](https://github.com/bartekczyz/claude-profiles/commit/dea9c33b97f6aa97103e91111b9cabafbce1575d))
* **ui:** polish onboarding controls ([1154350](https://github.com/bartekczyz/claude-profiles/commit/1154350296b57c0f355b0524555320621a8f7648))
* **updater:** stop overriding the platform key to darwin-universal ([cf5339b](https://github.com/bartekczyz/claude-profiles/commit/cf5339bcb6780aea875f6ba05b6ec97881b4ede2))


### Changed

* restructure as Turborepo monorepo under apps/claude-profiles ([7fa4f59](https://github.com/bartekczyz/claude-profiles/commit/7fa4f59aa79f22ac266426a108213589977fef0d))
* **tokens:** extract design tokens to packages/design-tokens ([3f327be](https://github.com/bartekczyz/claude-profiles/commit/3f327bea27147a129447593c166ef69cdb179f02))

## [0.2.0](https://github.com/bartekczyz/claude-profiles/compare/v0.1.1...v0.2.0) (2026-05-24)


### Added

* **migration:** redesign import dialog with side-by-side layout ([5cc2d25](https://github.com/bartekczyz/claude-profiles/commit/5cc2d256e5732522aed0aeec966dfe148d59f4c6))
* onboarding & migration polish ([0314d9b](https://github.com/bartekczyz/claude-profiles/commit/0314d9b38ef26c3704464c10b0f07c5145267c23))
* **onboarding:** replace auto-migration prompt with a fork dialog ([c2b6380](https://github.com/bartekczyz/claude-profiles/commit/c2b638062ff1c9418fbe71d6a1c24797832e5273))


### Fixed

* **onboarding:** restyle welcome dialog with the Atelier primitive ([8c8b110](https://github.com/bartekczyz/claude-profiles/commit/8c8b110c784a069e69a7f644b54707365116c567))
* **settings:** keep settings open when re-import is dismissed ([dea9c33](https://github.com/bartekczyz/claude-profiles/commit/dea9c33b97f6aa97103e91111b9cabafbce1575d))
* **ui:** polish onboarding controls ([1154350](https://github.com/bartekczyz/claude-profiles/commit/1154350296b57c0f355b0524555320621a8f7648))
* **updater:** stop overriding the platform key to darwin-universal ([cf5339b](https://github.com/bartekczyz/claude-profiles/commit/cf5339bcb6780aea875f6ba05b6ec97881b4ede2))

## 0.1.1 (2026-05-24)

Initial release.

## Changelog

All notable changes are recorded here. This file is maintained automatically by
[release-please](https://github.com/googleapis/release-please) — please don't edit it by hand.
Commit messages following the [Conventional Commits](https://www.conventionalcommits.org/)
spec drive what lands here on the next release.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
