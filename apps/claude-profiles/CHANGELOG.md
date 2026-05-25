# Changelog

## [0.2.2](https://github.com/bartekczyz/claude-profiles/compare/v0.3.0...v0.2.2) (2026-05-25)


* release 0.1.1 ([8dfbb73](https://github.com/bartekczyz/claude-profiles/commit/8dfbb73423d39595a644d6a4deaf4a9c68cb7287))


### Added

* **app:** focus sidebar search with ⌘F ([e74d248](https://github.com/bartekczyz/claude-profiles/commit/e74d2489fea67c327344b58de324a2e15a22bd8c))
* **empty-state:** guide users through Claude install when neither surface is detected ([b46aff6](https://github.com/bartekczyz/claude-profiles/commit/b46aff6e79f32e9e548d1ee542f2b2ad4f35b6b1))
* **empty-state:** guide users through Claude install when neither surface is detected ([2e922e1](https://github.com/bartekczyz/claude-profiles/commit/2e922e14b348b243fcf60f0d1f7d84ac1c2adec2))
* **migration:** redesign import dialog with side-by-side layout ([5cc2d25](https://github.com/bartekczyz/claude-profiles/commit/5cc2d256e5732522aed0aeec966dfe148d59f4c6))
* onboarding & migration polish ([0314d9b](https://github.com/bartekczyz/claude-profiles/commit/0314d9b38ef26c3704464c10b0f07c5145267c23))
* **onboarding:** replace auto-migration prompt with a fork dialog ([c2b6380](https://github.com/bartekczyz/claude-profiles/commit/c2b638062ff1c9418fbe71d6a1c24797832e5273))
* **settings:** show the app version next to the Updates row ([aa1054d](https://github.com/bartekczyz/claude-profiles/commit/aa1054d7118512fa663768b34a32cef5cc3b472f))


### Fixed

* **about:** correct version display and tidy About / Settings ([6bf20fe](https://github.com/bartekczyz/claude-profiles/commit/6bf20fe07a70049ac74129c6f3d6748d6ead7d30))
* **about:** use a clearer dialog title and drop the long subtitle ([ef4dd0d](https://github.com/bartekczyz/claude-profiles/commit/ef4dd0d93a126b36274900f048db9627dcfa890d))
* **boot:** paint brand background + loader before the JS bundle runs ([0d32ff3](https://github.com/bartekczyz/claude-profiles/commit/0d32ff30701a2c25066c24938433669aef25d173))
* **onboarding:** restyle welcome dialog with the Atelier primitive ([8c8b110](https://github.com/bartekczyz/claude-profiles/commit/8c8b110c784a069e69a7f644b54707365116c567))
* **settings:** keep settings open when re-import is dismissed ([dea9c33](https://github.com/bartekczyz/claude-profiles/commit/dea9c33b97f6aa97103e91111b9cabafbce1575d))
* **ui:** polish onboarding controls ([1154350](https://github.com/bartekczyz/claude-profiles/commit/1154350296b57c0f355b0524555320621a8f7648))
* **updater:** relaunch the app after downloadAndInstall ([689b6c1](https://github.com/bartekczyz/claude-profiles/commit/689b6c18e74b03999e63a201632f3012632a798b))
* **updater:** relaunch the app after downloadAndInstall ([7bfd98f](https://github.com/bartekczyz/claude-profiles/commit/7bfd98fbd5a3faf0cde4d3c170a3864db66e2c4d))
* **updater:** stop overriding the platform key to darwin-universal ([cf5339b](https://github.com/bartekczyz/claude-profiles/commit/cf5339bcb6780aea875f6ba05b6ec97881b4ede2))


### Performance

* **deps:** cache shell-PATH lookup and short-circuit via process env ([7b2135e](https://github.com/bartekczyz/claude-profiles/commit/7b2135e18c9b254fe35072fac690e8b92dfe6f4e))
* faster, less ugly app startup ([19225da](https://github.com/bartekczyz/claude-profiles/commit/19225da82f6fe6443f0afee4d12467378ed9cd89))
* **migration:** defer the directory-size walks off the boot path ([718fe9e](https://github.com/bartekczyz/claude-profiles/commit/718fe9efb0b8fb4fc70b1074df29ba2d014cfa98))


### Changed

* restructure as Turborepo monorepo under apps/claude-profiles ([7fa4f59](https://github.com/bartekczyz/claude-profiles/commit/7fa4f59aa79f22ac266426a108213589977fef0d))
* **tokens:** extract design tokens to packages/design-tokens ([3f327be](https://github.com/bartekczyz/claude-profiles/commit/3f327bea27147a129447593c166ef69cdb179f02))

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
