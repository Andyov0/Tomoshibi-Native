// The phrases the Android app says, in its four languages, from the same
// catalog the Apple apps build from.
//
//   node android/i18n.mjs
//
// apple/i18n/generate.mjs already joins the web's dictionaries and the
// phrases only the native apps say (apple/i18n/native.json) into one String
// Catalog, checking that every phrase is in all four languages and that every
// placeholder survives translation. Android reads that result rather than
// joining the sources a second time, so the two apps cannot drift into
// different words for the same thing: a phrase is added to native.json (or
// the web), apple/i18n/generate.mjs is run, then this.
//
// The output is one JSON object per language, keyed by the English, which is
// what t() looks up (basics/Localization.kt); committed, like the catalog.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const catalog = join(here, "../apple/TomoshibiKit/Sources/TomoshibiBasics/Resources/Localizable.xcstrings");
const out = join(here, "app/src/main/assets/phrases.json");
const languages = ["en", "zh-Hans", "zh-Hant", "ja"];

const { strings } = JSON.parse(readFileSync(catalog, "utf8"));
const tables = Object.fromEntries(languages.map((l) => [l, {}]));
for (const key of Object.keys(strings).sort()) {
	const localizations = strings[key].localizations ?? {};
	for (const language of languages) {
		const value = localizations[language]?.stringUnit?.value;
		if (value !== undefined && value !== key) tables[language][key] = value;
	}
}
writeFileSync(out, JSON.stringify(tables, null, "\t") + "\n");
console.log(languages.map((l) => `${l} ${Object.keys(tables[l]).length}`).join(", "), "->", out);
