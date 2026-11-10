# Contributing

Bug reports and suggestions go in the [issues](https://github.com/IoSonoPiero/eyecansee/issues). Everything in the repository (code, comments, commits, tests, documentation) is in English.

## Adding a language

1. Copy `locales/en.ftl` to `locales/<code>.ftl` (`<code>` = the two-letter language code, e.g. `de`) and translate every message. Keep the ids and the `{ $variables }` as they are, and put the name of the language in its own language in `language-name`.
2. Copy the guide, `docs/help/en/`, to `docs/help/<code>/` and translate every page, `index.md` included. Keep the same files and the same structure: each page starts with its title (`# Title`) and has the same headings, at the same levels and in the same order, as the English page. `index.md` starts with the translated title of the contents page, has one `## <area>` per area and, under each, one line `- [<title>](<page>.md)` per page, in the English order, with each title exactly as the first line of its page. Write the names of buttons and menus as the translated interface shows them, also in the icons (`![Save](icon://save)`). The pictures in `docs/help/img/` stay in English, the same for every language: keep every `![…](../img/<name>.webp)` and every `<!-- shot: … -->` line where it is, translating only the text.
3. Add the language in the code:
   - `src/i18n.rs`: one line in `LANGS`, and the language in the list that the `languages_in_sync` test expects from `languages()`:
     ```rust
     ("<code>", include_str!("../locales/<code>.ftl")),
     ```
   - `src/help.rs`: one line in the `pages!` macro, which embeds the pages in the program:
     ```rust
     $(("<code>", $id, include_str!(concat!("../docs/help/<code>/", $id, ".md"))),)*
     ```
   - `src/help_check.rs`: the language and the title of its contents page in `LANGS`, and the translated name of each area in `CONTRACT` (`names` gets one more entry).
4. Run `cargo test`. It checks that every language has the same message ids as English, and that the guide is consistent: every language has exactly the same pages, each page starts with its title and has the same headings as the English one, with nothing deeper than `###`; the contents page lists every page once, in the right areas and order, under its real title; links lead to existing pages (`<page>.md`, without `#` anchors), pictures to files in `docs/help/img/`, icons to icons the program has, and other links are `https://`; every page shows the same pictures and placeholders as the English one; every picture is shown by some page; the pages and pictures embedded in the program are exactly the files of the guide; and every F1 spot in the code (`"help:<page>"`) has its page in every language.
5. Try it: `EYECANSEE_LANG=<code> cargo run -- ~/Pictures`, and press F1.

English is the reference and the fallback: a message missing from a translation is shown in English, and the guide is shown in English for a language that has no pages.
