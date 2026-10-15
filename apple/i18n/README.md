# Phrases

The apps speak English, simplified Chinese, traditional Chinese and Japanese,
the same four as the web client and in the same words. The web's dictionaries
(`web/src/live/dictionaries/` in the server repository) are the source; this
folder turns them into the String Catalog the apps build from.

## The rule

Every phrase the interface shows goes through `t()`, keyed by its English:

```swift
Text(t("Show {name} larger", ["name": person.name]))
```

and every phrase is in all four languages. English is the key as well as the
value, so a phrase with no translation still reads as a sentence rather than a
dotted path. Placeholders are `{likeThis}`, ASCII letters, digits and
underscores, exactly as the web writes them; a translation keeps every one of
them, unchanged, wherever its own word order puts it. Values are filled in after
translation, never concatenated around translated fragments.

The server sends error codes, and the client owns the sentence: the app maps
each code to a phrase and says it through `t()`, and never shows text the server
wrote.

Text the deployment supplies rather than the interface, such as a relay's label,
goes through `say()` instead, which translates it if it matches a phrase and
otherwise shows it as typed. Nothing else does.

## Adding a phrase

A phrase the web already says needs nothing: it is in the catalog. A phrase
only the apps say goes in `native.json`:

```json
"phrases": {
  "Quit Tomoshibi": {
    "en": "Quit Tomoshibi",
    "zh-Hans": "退出 Tomoshibi",
    "zh-Hant": "結束 Tomoshibi",
    "ja": "Tomoshibi を終了"
  }
}
```

Then regenerate.

## Regenerating

```sh
node apple/i18n/generate.mjs           # write the catalog
node apple/i18n/generate.mjs --check   # fail if the catalog is out of date
```

Node 22.18 or later, which runs the web's `.ts` dictionaries directly. The
server repository is looked for beside this one, as `tomoshibi-relay`; set
`TOMOSHIBI_RELAY=/path/to/it` otherwise. Regenerate after the web's dictionaries
change, and commit the catalog with whatever made it change.

The generator stops, writes nothing, and lists every problem when:

- a web dictionary is missing a phrase the English has, or has one it does not;
- a phrase in `native.json` lacks a language, gives one the apps do not speak,
  is written twice, or is already one of the web's;
- a translation's placeholders differ from its phrase's;
- the web has gained or lost a language.

The output is sorted, so regenerating from unchanged dictionaries writes the
same bytes and a diff shows only what changed.

`Resources/Localizable.xcstrings` is generated. Do not edit it, and do not let
Xcode's String Catalog editor save it; change the web's dictionaries or
`native.json` and regenerate.

## What checks it

`Tests/TomoshibiKitTests/LocalizationCatalogTests.swift` holds the catalog and
the tables the build makes of it to the same rules, and reads the Swift sources
under `TomoshibiKit/Sources` and `App` for calls to `t()` whose phrase is not in
the catalog. On the web the compiler does that last part; in Swift `t()` takes a
`String`, so a phrase that is misspelt or missing from `native.json` fails that
test instead of showing English to everybody.
