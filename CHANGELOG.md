# Changelog

## [0.1.3](https://github.com/mpecan/rable/compare/rable-v0.1.2...rable-v0.1.3) (2026-03-24)


### Bug Fixes

* sync pyproject.toml version to 0.1.1 and fix release-please config ([5efe677](https://github.com/mpecan/rable/commit/5efe6778bcec782479a1f13eec283872497b6e02))

## [0.1.2](https://github.com/mpecan/rable/compare/rable-v0.1.1...rable-v0.1.2) (2026-03-24)


### Bug Fixes

* **ci:** pin Python 3.13 for wheel builds ([f1f4609](https://github.com/mpecan/rable/commit/f1f4609a1aa64ad65322f9712b68e0ff65e3cb7f))

## [0.1.1](https://github.com/mpecan/rable/compare/rable-v0.1.0...rable-v0.1.1) (2026-03-24)


### Features

* achieve 100% Parable compatibility (1604/1604) ([4d8b379](https://github.com/mpecan/rable/commit/4d8b379309c298e664d4a66f73489da4099c75be))
* implement PyO3 Python bindings (Phase 9) ([b22a53e](https://github.com/mpecan/rable/commit/b22a53e865f2ca3f92fb2765b777acbe177d9af9))
* implement WordSegment state machine for word value processing ([05b308c](https://github.com/mpecan/rable/commit/05b308cdaf060ade3d8b90dfa31622779fc5f7e4))
* initial Rable implementation — Rust bash parser at 93.6% Parable compatibility ([b52e0bc](https://github.com/mpecan/rable/commit/b52e0bcc3573b8a29883c17e629edb7e65dd736d))
* line continuation support, heredoc improvements ([35479a9](https://github.com/mpecan/rable/commit/35479a9e7749e313de1a8d23e7d40ad36234911c))
* push Parable compatibility from 97.3% to 99.6% (1597/1604) ([ffd0415](https://github.com/mpecan/rable/commit/ffd04156a282d396b69241afe1071d5ae0610cc9))


### Bug Fixes

* & precedence, heredoc double-backslash, coproc redirects ([1951a2f](https://github.com/mpecan/rable/commit/1951a2f06d6e3677d8d7ee4dccd7a7a0a893614e))
* ANSI-C quoting escapes, control chars, redirect processing ([544447b](https://github.com/mpecan/rable/commit/544447b33f99beb77705ee42a204dba1a6c6336d))
* **ci:** create venv for maturin develop in Python and Benchmark jobs ([1b7be5e](https://github.com/mpecan/rable/commit/1b7be5ea2591e0dd0bfebcece139c6c599a0db18))
* comment handling in matched parens, escaped $ protection ([1fbc396](https://github.com/mpecan/rable/commit/1fbc396228e16561941ae5e16933ffaac8cca2d9))
* comment handling, line continuation in parens ([c74fd2b](https://github.com/mpecan/rable/commit/c74fd2b4cd8b6c580460a095a484a167f8f1552c))
* conditional expressions, cstyle-for defaults, precedence ([9f1ff9c](https://github.com/mpecan/rable/commit/9f1ff9c13514bdec483670f0e3f828e945f9d0b6))
* conditional formatting in cmdsub, locale/ANSI-C in redirects ([b2dc5a1](https://github.com/mpecan/rable/commit/b2dc5a1d2177cb822db6baf1edfdb2fa16f57129))
* coproc redirects, locale in redirects, arith spacing ([574d21f](https://github.com/mpecan/rable/commit/574d21fbf60bde68848639633a5c43fb256ac0c5))
* line continuation in matched parens, ANSI-C control chars ([0cbb912](https://github.com/mpecan/rable/commit/0cbb9125e20ec7b0fd139d12b9d4f7e1fc4369ae))
* negation/time nesting, arithmetic $(()), brace/select edge cases ([35d52bf](https://github.com/mpecan/rable/commit/35d52bfd761dfa29683315278d680d096af1dd61))
* revert top-level unwrap (caused 26 regressions), cleanup ([df57600](https://github.com/mpecan/rable/commit/df57600763117c46add6bedf6ffad68fced19f02))


### Documentation

* add README, LICENSE, CONTRIBUTING, justfile, and benchmark ([a4fab19](https://github.com/mpecan/rable/commit/a4fab19a46dec3582b2d5ad11297585973bd9108))


### Code Refactoring

* move lexer, parser, sexp, format into module directories ([8608bff](https://github.com/mpecan/rable/commit/8608bffd9e94a143b3999c39cdcc3ba95d0c5b27))
* replace ParserStateFlags with encapsulated lexer state API ([7d7242d](https://github.com/mpecan/rable/commit/7d7242d6dbf2be2678f30ff92f7301f76ef05335))
* split lexer and parser tests into sub-modules ([3296f1e](https://github.com/mpecan/rable/commit/3296f1ec865b89256e6fde841a74f9962aa062ab))
* split parser into 5 sub-modules ([cb5a853](https://github.com/mpecan/rable/commit/cb5a853f8b8d5c0d46f96d3ea9ae14f00110771f))
