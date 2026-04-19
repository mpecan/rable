# Changelog

## [0.2.0](https://github.com/mpecan/rable/compare/rable-v0.1.15...rable-v0.2.0) (2026-04-18)


### ⚠ BREAKING CHANGES

* tighten lexer API surface and relocate WordSpan to ast ([#70](https://github.com/mpecan/rable/issues/70))

### Bug Fixes

* **format:** align cmdsub reformatter with bash canonical form ([#49](https://github.com/mpecan/rable/issues/49)) ([c7a4411](https://github.com/mpecan/rable/commit/c7a4411f8628a7a855a550dfe126e070812f108a))
* **lexer:** accept sloppy heredoc terminator in cmdsub mode ([#50](https://github.com/mpecan/rable/issues/50)) ([40f394f](https://github.com/mpecan/rable/commit/40f394fa8bf7d88286ba74d5a57eaffc50cb6b0d))
* **lexer:** backticks opaque when content is invalid ([#71](https://github.com/mpecan/rable/issues/71)) ([e72166f](https://github.com/mpecan/rable/commit/e72166ff2ab398e4dba7457255487cb448f9dca3)), closes [#38](https://github.com/mpecan/rable/issues/38)
* **lexer:** disable reserved-word recognition after assignment words ([#44](https://github.com/mpecan/rable/issues/44)) ([42e1fc0](https://github.com/mpecan/rable/commit/42e1fc0365667cac9f24c4ecc15a980e890bb3d4))
* **lexer:** stop treating ]] and unbalanced [...] as special outside conditionals ([#45](https://github.com/mpecan/rable/issues/45)) ([4bf5a5c](https://github.com/mpecan/rable/commit/4bf5a5c13fbb08d9ce7ab4b27fec44627a589116))
* **parser:** fall back from (( … )) arith to nested subshells ([#48](https://github.com/mpecan/rable/issues/48)) ([1437f00](https://github.com/mpecan/rable/commit/1437f00b64d76f372f3851d3d3c044daf8b51c6f))


### Code Refactoring

* **format:** introduce Formatter struct ([#65](https://github.com/mpecan/rable/issues/65)) ([d965a8f](https://github.com/mpecan/rable/commit/d965a8fd30a895de43f07075b6aa31c56acfb28c))
* **lexer:** drop Result&lt;Token&gt; wrapper from operator readers ([#62](https://github.com/mpecan/rable/issues/62)) ([d52a841](https://github.com/mpecan/rable/commit/d52a841a2191357120fbf5953e6a4effb042abcc))
* **lexer:** split read_word_token into classify + advance + dispatch helpers ([#63](https://github.com/mpecan/rable/issues/63)) ([3ba09f5](https://github.com/mpecan/rable/commit/3ba09f51f9b607baf4bc3d407cc17f439982e27b))
* **parser:** extract fill_heredoc_contents visitor helpers ([#68](https://github.com/mpecan/rable/issues/68)) ([40e6165](https://github.com/mpecan/rable/commit/40e616504db6e4f20b8e00ecd0da29aa588e0968))
* **parser:** extract helpers from three oversize parsers ([#69](https://github.com/mpecan/rable/issues/69)) ([25d0762](https://github.com/mpecan/rable/commit/25d076219d2f228b3de0efde74eecc2e110576fb))
* **sexp:** dispatch NodeKind Display to per-category helpers ([#66](https://github.com/mpecan/rable/issues/66)) ([44b0330](https://github.com/mpecan/rable/commit/44b033050262d8434232cbd210698cde22d656be))
* **sexp:** table-drive ANSI-C escape dispatch ([#67](https://github.com/mpecan/rable/issues/67)) ([91a5267](https://github.com/mpecan/rable/commit/91a52673cdcc5b499e818dd89a5acfeb00795c11))
* tighten lexer API surface and relocate WordSpan to ast ([#70](https://github.com/mpecan/rable/issues/70)) ([5171d01](https://github.com/mpecan/rable/commit/5171d0145ec9a545b5ed3a02cc66c9de90d780d8))

## [0.1.15](https://github.com/mpecan/rable/compare/rable-v0.1.14...rable-v0.1.15) (2026-04-14)


### Code Refactoring

* **lexer:** parse $(...) via fork-and-merge instead of a sub-lexer ([34f65ad](https://github.com/mpecan/rable/commit/34f65ad99000762242394a68a3e0bff26cf826da))
* **lexer:** parse $(...) via fork-and-merge instead of a sub-lexer ([#29](https://github.com/mpecan/rable/issues/29)) ([1399f90](https://github.com/mpecan/rable/commit/1399f90489c310678f51d0f54884f13d1d5059e0))
* **lexer:** parse backticks via fork-and-merge ([b41ffec](https://github.com/mpecan/rable/commit/b41ffecf5fe8b27b2b5c696a62d68e491566e210)), closes [#30](https://github.com/mpecan/rable/issues/30)
* **lexer:** parse backticks via fork-and-merge ([#33](https://github.com/mpecan/rable/issues/33)) ([b1c99d9](https://github.com/mpecan/rable/commit/b1c99d9c48932ef8716a5c90db910c9586bc77eb))
* **lexer:** parse process substitution via fork-and-merge ([26ea44b](https://github.com/mpecan/rable/commit/26ea44b7f4667fce0b6e09a34bf5ae415d9eec48)), closes [#31](https://github.com/mpecan/rable/issues/31)
* **lexer:** parse process substitution via fork-and-merge ([#34](https://github.com/mpecan/rable/issues/34)) ([4ce7fe9](https://github.com/mpecan/rable/commit/4ce7fe95073b850d4fc1d4901bdeb3b4d8a0f172))

## [0.1.14](https://github.com/mpecan/rable/compare/rable-v0.1.13...rable-v0.1.14) (2026-04-13)


### Bug Fixes

* **lexer:** skip heredoc bodies in command substitution paren tracking ([b547bf6](https://github.com/mpecan/rable/commit/b547bf6ed66b7d0638b3787a9278383790131730)), closes [#26](https://github.com/mpecan/rable/issues/26)
* **lexer:** skip heredoc bodies in command substitution paren tracking ([#27](https://github.com/mpecan/rable/issues/27)) ([82a0721](https://github.com/mpecan/rable/commit/82a0721875f390a28309ad7f68c4dca7c21db7ac))

## [0.1.13](https://github.com/mpecan/rable/compare/rable-v0.1.12...rable-v0.1.13) (2026-04-12)


### Features

* decompose backtick command substitutions into typed AST nodes ([330150b](https://github.com/mpecan/rable/commit/330150be817d3cc8a2c3c340ed4adbab07c11879))
* decompose backtick command substitutions into typed AST nodes ([#24](https://github.com/mpecan/rable/issues/24)) ([627f5ac](https://github.com/mpecan/rable/commit/627f5acfb726f061ad9b173bb5f695f8afc05c98))

## [0.1.12](https://github.com/mpecan/rable/compare/rable-v0.1.11...rable-v0.1.12) (2026-04-09)


### Code Refactoring

* enforce 500/700 file length limits and decompose large modules ([#22](https://github.com/mpecan/rable/issues/22)) ([0b317c5](https://github.com/mpecan/rable/commit/0b317c5860b81a98d31b64d221dfa420f1756be4))
* **format:** split mod.rs into topic helper files ([4218a61](https://github.com/mpecan/rable/commit/4218a6173a261b8edd375b74f96f62b6bb2309a9))
* **parser:** split arithmetic.rs into topic submodules ([0959cfa](https://github.com/mpecan/rable/commit/0959cfa47a1a3343801ba45013d00f286ccc62da))
* **parser:** split mod.rs and compound.rs into topic files ([6c74954](https://github.com/mpecan/rable/commit/6c749544c17ab07a396bca3dc9b930ae804a163f))
* **parser:** split word_parts.rs into topic submodules ([54a796f](https://github.com/mpecan/rable/commit/54a796f860b8775a0285d0174c13fdd697408094))
* **sexp:** split mod.rs into topic helper files ([8d1a4e2](https://github.com/mpecan/rable/commit/8d1a4e24965f02541b0f919a2f59f262ab8108e7))

## [0.1.11](https://github.com/mpecan/rable/compare/rable-v0.1.10...rable-v0.1.11) (2026-04-09)


### Features

* decompose arithmetic, ANSI-C, and locale expansions in Word.parts ([9887e56](https://github.com/mpecan/rable/commit/9887e56cde877f470a563694ed0820ba5bce23c6))
* decompose arithmetic, ANSI-C, and locale expansions in Word.parts ([#20](https://github.com/mpecan/rable/issues/20)) ([95299e9](https://github.com/mpecan/rable/commit/95299e96d7c742ce4c79b4a2e040b3c5173de7ce))

## [0.1.10](https://github.com/mpecan/rable/compare/rable-v0.1.9...rable-v0.1.10) (2026-04-07)


### Features

* recognize brace expansion patterns in Word.parts ([fef69dc](https://github.com/mpecan/rable/commit/fef69dce65847c3e67059f97ec44d5bacaac9db6))
* recognize brace expansion patterns in Word.parts ([#18](https://github.com/mpecan/rable/issues/18)) ([c9fd580](https://github.com/mpecan/rable/commit/c9fd580d379cd518dbf8d33e633e29be5fa90246))

## [0.1.9](https://github.com/mpecan/rable/compare/rable-v0.1.8...rable-v0.1.9) (2026-04-07)


### Features

* decompose parameter expansions into Word.parts ([b0ad648](https://github.com/mpecan/rable/commit/b0ad64831b8fdafee3d466fa4193b720c742185c))
* decompose parameter expansions into Word.parts ([#17](https://github.com/mpecan/rable/issues/17)) ([ded6897](https://github.com/mpecan/rable/commit/ded6897868f4dab44170c5a3a01052fcb6b6e976))


### Bug Fixes

* resolve clippy errors in tree-sitter comparison test ([789da4d](https://github.com/mpecan/rable/commit/789da4d1b39a6afc8776fc73dd7933fcad9b4abe))
* resolve clippy errors in tree-sitter comparison test ([#15](https://github.com/mpecan/rable/issues/15)) ([26570f9](https://github.com/mpecan/rable/commit/26570f9968ac94e68a9534550db956ce94aff382))

## [0.1.8](https://github.com/mpecan/rable/compare/rable-v0.1.7...rable-v0.1.8) (2026-03-25)


### Features

* enrich AST with structured word spans and assignment detection ([9163d24](https://github.com/mpecan/rable/commit/9163d24ecf5472da8238324ef6f7ff3a55bcf3ba))
* enrich AST with structured word spans and assignment detection ([#11](https://github.com/mpecan/rable/issues/11)) ([3b58c38](https://github.com/mpecan/rable/commit/3b58c38180220563bb28e5656dc4d49263a94584))


### Bug Fixes

* CTLESC byte doubling for bash-oracle compatibility (179/181) ([72bc381](https://github.com/mpecan/rable/commit/72bc38192a25e034cb8d925e46b7ebfcbb9d7cf1))
* heredoc trailing newline at EOF with backslash (180/181) ([4af8d91](https://github.com/mpecan/rable/commit/4af8d919b39792e3bdd134294639f18de3d46478))
* resolve 11 oracle test failures (180/181) ([#13](https://github.com/mpecan/rable/issues/13)) ([69d6bc8](https://github.com/mpecan/rable/commit/69d6bc81a2cf834bed3de44ed6e190fb70d11d09))
* resolve 3 more oracle failures (177/181) ([8aca953](https://github.com/mpecan/rable/commit/8aca95341ae09182e1535e9521c283fb85f04f61))
* resolve 6 oracle test failures ([0496222](https://github.com/mpecan/rable/commit/049622265c85b4cd788d307ed8465406699d400d))
* resolve 6 oracle test failures (175/181) ([1708884](https://github.com/mpecan/rable/commit/17088844f9c4c0b381dc737041f3e48bec51a02e))


### Documentation

* comprehensive documentation update ([#14](https://github.com/mpecan/rable/issues/14)) ([6abfb20](https://github.com/mpecan/rable/commit/6abfb205768b43b444dd759cda1e1411b68bc3ca))
* comprehensive documentation update for better DX ([61114b0](https://github.com/mpecan/rable/commit/61114b043e28c75b0809d13fac48395dd9959f86))


### Code Refactoring

* remove sexp re-parsing by threading spans through all nodes ([54db8c7](https://github.com/mpecan/rable/commit/54db8c79d15689d475ec12aeadc2d28b2d62fa90))
* simplify span collection, move to owned tokens, remove dead code ([cec7e8e](https://github.com/mpecan/rable/commit/cec7e8e5f8a245b15211598f21f1b5fedd6081d4))

## [0.1.7](https://github.com/mpecan/rable/compare/rable-v0.1.6...rable-v0.1.7) (2026-03-25)


### Features

* populate source spans on all AST nodes ([0e32c37](https://github.com/mpecan/rable/commit/0e32c379607940bd8d083ca4670b04f8980dd443))
* populate source spans on all AST nodes ([#8](https://github.com/mpecan/rable/issues/8)) ([a8c92e8](https://github.com/mpecan/rable/commit/a8c92e8dd6a8fe7425edd94fcc647492bc3cd32d))
* populate source spans on all AST nodes ([#9](https://github.com/mpecan/rable/issues/9)) ([b8be160](https://github.com/mpecan/rable/commit/b8be160b4a397a552259c7bb72b07e9519f0df54))

## [0.1.6](https://github.com/mpecan/rable/compare/rable-v0.1.5...rable-v0.1.6) (2026-03-25)


### Features

* enrich AST with spans, structured lists, and pipe separators ([#7](https://github.com/mpecan/rable/issues/7)) ([53269b7](https://github.com/mpecan/rable/commit/53269b7b2de1c0bc7fe6dd49255509d638bc58ac))
* enrich AST with spans, structured lists, pipe separators, and assignments ([7eec43e](https://github.com/mpecan/rable/commit/7eec43e4ecd415c0ca16ab36f6e5c79883bf6142))
* improve Rust developer experience ([2ac7654](https://github.com/mpecan/rable/commit/2ac7654132f49eedddc62433e50195dc26b13b3f))


### Bug Fixes

* trailing list operators in format and assert on Parable compat failures ([3a98be1](https://github.com/mpecan/rable/commit/3a98be1ad1e7c4461974041d153edb41b253f506))

## [0.1.5](https://github.com/mpecan/rable/compare/rable-v0.1.4...rable-v0.1.5) (2026-03-24)


### Features

* add differential fuzzer for bash-oracle and Parable comparison ([4d18eff](https://github.com/mpecan/rable/commit/4d18effb0a75d7e87f5fdd44ed8d40560ee3b209))
* bracket subscript tracking in lexer (112/181 oracle) ([35016dc](https://github.com/mpecan/rable/commit/35016dcb4f24abca68439f83252961b7e74e4ac7))
* generate 186 oracle-derived tests from bash-oracle fuzzing ([e7940ce](https://github.com/mpecan/rable/commit/e7940ce55c4056fac648593ef62cbffd24a9a7cf))
* process sub word continuation + continue_word helper (124/181) ([2caaf53](https://github.com/mpecan/rable/commit/2caaf53d0437b6886c3d47d087ce36018b695e56))


### Bug Fixes

* &&gt; and &&gt;&gt; redirects never accept fd number prefix (72/181) ([e93a6d4](https://github.com/mpecan/rable/commit/e93a6d4a7f6df4a58296367ad2730b4b6b0f36c9))
* &gt;&- and &lt;&- close-fd as complete lexer operators (136/181) ([e5f4f0e](https://github.com/mpecan/rable/commit/e5f4f0ea00e5e0e38bdcbdec1309341809831538))
* $\" inside double quotes is bare dollar, not locale string (165/181) ([a80b648](https://github.com/mpecan/rable/commit/a80b648763d1ef949ffaa886fa9fbb240cade236))
* broaden reformatter gate + trim trailing spaces (94/181) ([06d2d93](https://github.com/mpecan/rable/commit/06d2d93441437ae7a64b20dcdbbf8e8bd44a5f8c))
* cmdsub in redirect targets + close-fd formatting (131/181) ([2d5b8a8](https://github.com/mpecan/rable/commit/2d5b8a883c910da424ce33d69e4b9fa57b5cd48e))
* escape backslashes in arithmetic S-expression output (64/181) ([3e1d41e](https://github.com/mpecan/rable/commit/3e1d41e216f286783727414cdf63006b7e9a25e5))
* fd numbers only recognized adjacent to redirect operators (78/181) ([f5962e4](https://github.com/mpecan/rable/commit/f5962e440f4bfeb655ee15285f530752b83992ca))
* high-byte ANSI-C replacement chars + locale double-quote gating (83/181) ([118cb81](https://github.com/mpecan/rable/commit/118cb81bafd91dbb736e5a5fd2546c09d98a1cb3))
* improve ANSI-C processing and bash-oracle compat (60/181) ([dd28ade](https://github.com/mpecan/rable/commit/dd28ade754d847993654fe7e61ce385528d8ed06))
* improve bash-oracle compatibility (37/181 oracle tests passing) ([d1c07fb](https://github.com/mpecan/rable/commit/d1c07fbe0f7f0b37b2e764a2b746424938f36646))
* improve bash-oracle compatibility (48/181 oracle tests passing) ([2ee0ea1](https://github.com/mpecan/rable/commit/2ee0ea10b6486d83e67d3986024c6ef114012630))
* pipeline time demoting, move-fd quoting, reserved words (144/181) ([27a9c94](https://github.com/mpecan/rable/commit/27a9c9422c127c2fce70d26bf2334a5aea04e161))
* procsub word continuation + double-quote context gating (127/181) ([da0eba1](https://github.com/mpecan/rable/commit/da0eba1420f7f4677a1b161c60dda5b55dac6a94))
* single-quoted regions opaque to word segment parser (160/181) ([a9ca786](https://github.com/mpecan/rable/commit/a9ca786674dc8059da9d3959a38d8f9793b660da))
* time after | in pipelines treated as regular word (139/181) ([7e6b592](https://github.com/mpecan/rable/commit/7e6b59211ef4dd0ee3909b995fa46785d88dca6d))
* update fuzzer to use --dump-ast flag for bash-oracle ([b335332](https://github.com/mpecan/rable/commit/b335332f93e007d57196f5af1eeef3b394482346))
* varfd adjacency + digit-only varfd rejection (141/181) ([2ba8788](https://github.com/mpecan/rable/commit/2ba87882df404abb102bdfedc077bd671c3b94cd))


### Code Refactoring

* centralize quote skipping and balanced delimiter reading ([dc312ca](https://github.com/mpecan/rable/commit/dc312ca38bb7b73a02a61bde4be597fbf45dcd6b))
* eliminate code duplication from cargo-dupes audit ([974a894](https://github.com/mpecan/rable/commit/974a894d7bef5ad3aa1ba46c0deed87ad112932a))
* extract shared CaseTracker and is_backslash_escaped ([3d774e1](https://github.com/mpecan/rable/commit/3d774e1446a7b356be5a5134c97cd7488435a7fc))
* Token::adjacent_to + depth guard increase to 2 ([1b1824d](https://github.com/mpecan/rable/commit/1b1824d7f136c01586343214db5217811905e6e6))
* unify process_word_value with parse_word_segments (149/181) ([816161b](https://github.com/mpecan/rable/commit/816161b93cb5cfa5dd448779b77db3c691abd53a))
* unify redirect word processing, eliminate feature envy (154/181) ([b520411](https://github.com/mpecan/rable/commit/b5204116e68d1762611de8633c9a736c7009ccdc))

## [0.1.4](https://github.com/mpecan/rable/compare/rable-v0.1.3...rable-v0.1.4) (2026-03-24)


### Bug Fixes

* include README.md as PyPI package description ([5c8cdc2](https://github.com/mpecan/rable/commit/5c8cdc214037d103eb99492c5bb4fa65c9210717))

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
