# Contributing

Bug reports and suggestions go in the [issues](https://github.com/IoSonoPiero/eyecansee/issues). Everything in the repository (code, comments, commits, tests, documentation) is in English.

## Adding a language

1. Copy `locales/en.ftl` to `locales/<code>.ftl` (`<code>` = the two-letter language code, e.g. `de`) and translate every message. Keep the ids and the `{ $variables }` as they are, and put the name of the language in its own language in `language-name`.
2. Copy `docs/manual.en.md` to `docs/manual.<code>.md` and translate it. Keep the same headings in the same order: the test compares them with the English manual.
3. Add one line to `LANGS` in `src/i18n.rs`:
   ```rust
   ("<code>", include_str!("../locales/<code>.ftl"), include_str!("../docs/manual.<code>.md")),
   ```
4. Run `cargo test`: it checks that every language has the same message ids and manual sections as English.
5. Try it: `EYECANSEE_LANG=<code> cargo run -- ~/Pictures`.

English is the reference and the fallback: a message missing from a translation is shown in English.
