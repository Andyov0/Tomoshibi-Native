#!/usr/bin/env node
/*
 * Writes the String Catalog the apps speak from.
 *
 * The phrases come from the web client's own dictionaries, read where the web
 * keeps them rather than from a copy, so that a translation corrected there
 * reaches the apps at the next regeneration instead of living on here in the
 * old wording. The few phrases only the apps say (menus, permission prompts)
 * come from native.json beside this file, in the same four languages.
 *
 * The web holds its dictionaries to a test that compares every one of them with
 * the English, in both directions, and checks that every placeholder survives
 * translation. The same checks run here, before anything is written, and any
 * failure stops the generator with every problem listed: a catalog with a hole
 * in it would not fail anywhere else, it would quietly show English to a reader
 * of Japanese.
 *
 * The output is sorted throughout, so that regenerating from unchanged
 * dictionaries writes the same bytes and a diff shows only real changes.
 *
 * The system's own prompts for the camera, the microphone and the local
 * network are the app's words too, and were English to everybody: they are
 * read from the app's Info.plist, which the build makes from project.yml in
 * English, and a second catalog, App/InfoPlist.xcstrings, gives each in the
 * other three languages. Its words are native.json's "infoPlist", keyed by
 * the Info.plist key, and the English there has to be project.yml's, word
 * for word, so that the two cannot drift apart.
 *
 *   node apple/i18n/generate.mjs           write both catalogs
 *   node apple/i18n/generate.mjs --check   fail if either is out of date
 *
 * The server repository is looked for beside this one, as tomoshibi-relay;
 * TOMOSHIBI_RELAY=/path/to/it points elsewhere.
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const relay = resolve(process.env.TOMOSHIBI_RELAY ?? join(here, "../../../tomoshibi-relay"));
const live = join(relay, "web/src/live");
const nativePath = join(here, "native.json");
const catalogPath = join(here, "../TomoshibiKit/Sources/TomoshibiBasics/Resources/Localizable.xcstrings");
const infoPlistPath = join(here, "../App/InfoPlist.xcstrings");
const projectPath = join(here, "../project.yml");

/*
 * The languages the apps are written in, in the order the web lists them. The
 * Swift side names the same four (Localization/Language.swift); the web's list
 * is read below and compared with this one, so that a fifth language added to
 * the web stops this generator rather than being silently left out.
 */
const LOCALES = ["en", "zh-Hans", "zh-Hant", "ja"];

const problems = [];

function fail(message) {
	problems.push(message);
}

function stop() {
	if (problems.length === 0) return;
	console.error(`generate.mjs: ${problems.length} problem(s); nothing was written.\n`);
	for (const problem of problems) console.error(`  ${problem}`);
	process.exit(1);
}

/** The names inside the braces of a phrase, sorted; the web's own pattern. */
function placeholders(template) {
	return [...template.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
}

/* The web's languages, from the declaration in i18n.ts. */

if (!existsSync(join(live, "i18n.ts"))) {
	console.error(`generate.mjs: no web client at ${live}. Set TOMOSHIBI_RELAY to the server repository.`);
	process.exit(1);
}

const declared = /export const LOCALES = \[([^\]]*)\]/.exec(readFileSync(join(live, "i18n.ts"), "utf8"));
const webLocales = declared ? [...declared[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]) : [];
if (webLocales.join(",") !== LOCALES.join(",")) {
	fail(
		`the web declares LOCALES as [${webLocales.join(", ")}], and this generator and the Swift side know [${LOCALES.join(", ")}]`,
	);
}
stop();

/*
 * The web's dictionaries, imported as modules. Node strips the TypeScript
 * annotations itself (`import type`, `as const`, `: Dictionary`), so what is
 * read is exactly what the web bundles: the object a browser looks phrases up
 * in, with any key written twice already resolved the way JavaScript resolves
 * it. The web's own test catches a key written twice in the source.
 */
const web = {};
for (const locale of LOCALES) {
	const path = join(live, "dictionaries", `${locale}.ts`);
	web[locale] = (await import(pathToFileURL(path).href)).default;
}

const english = Object.keys(web.en);
const englishSet = new Set(english);

for (const locale of LOCALES.filter((locale) => locale !== "en")) {
	const keys = Object.keys(web[locale]);
	const keySet = new Set(keys);

	for (const phrase of english) {
		if (!keySet.has(phrase)) fail(`web ${locale}: missing ${JSON.stringify(phrase)}`);
	}
	for (const phrase of keys) {
		if (!englishSet.has(phrase)) fail(`web ${locale}: says ${JSON.stringify(phrase)}, which English does not`);
	}
}

/*
 * The phrases only the apps say. JSON, like JavaScript, keeps the last of two
 * equal keys without a word, so the raw text is counted as well: a phrase added
 * twice with different translations would otherwise ship whichever came second.
 */
const nativeText = readFileSync(nativePath, "utf8");
let native;
try {
	native = JSON.parse(nativeText);
} catch (error) {
	console.error(`generate.mjs: ${relative(process.cwd(), nativePath)} is not valid JSON: ${error.message}`);
	process.exit(1);
}

const nativePhrases = native?.phrases;
if (typeof nativePhrases !== "object" || nativePhrases === null || Array.isArray(nativePhrases)) {
	fail(`native.json: "phrases" must be an object of phrase to translations`);
	stop();
}

for (const [phrase, translations] of Object.entries(nativePhrases)) {
	const quoted = JSON.stringify(phrase);
	const written = nativeText.split(new RegExp(`${quoted.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*:`)).length - 1;
	if (written > 1) fail(`native.json: ${quoted} is written ${written} times`);

	if (englishSet.has(phrase)) {
		fail(`native.json: ${quoted} is already in the web's dictionaries; say it there or here, not both`);
	}
	if (typeof translations !== "object" || translations === null || Array.isArray(translations)) {
		fail(`native.json: ${quoted} must map each language to its translation`);
		continue;
	}
	for (const locale of LOCALES) {
		if (typeof translations[locale] !== "string") fail(`native.json: ${quoted} has no ${locale}`);
	}
	for (const locale of Object.keys(translations)) {
		if (!LOCALES.includes(locale)) fail(`native.json: ${quoted} gives ${JSON.stringify(locale)}, which is not a language the apps speak`);
	}
}
stop();

/*
 * The Info.plist's words: every one in the four languages and no other, none
 * with a placeholder (the system fills nothing in), and its English the one
 * project.yml gives the key wherever it gives it.
 */
const infoPlist = native.infoPlist ?? {};
const project = readFileSync(projectPath, "utf8");
if (typeof infoPlist !== "object" || Array.isArray(infoPlist)) fail(`native.json: "infoPlist" must be an object of Info.plist key to translations`);
for (const [key, translations] of Object.entries(infoPlist)) {
	if (typeof translations !== "object" || translations === null || Array.isArray(translations)) {
		fail(`native.json: infoPlist ${key} must map each language to its translation`);
		continue;
	}
	for (const locale of LOCALES) {
		if (typeof translations[locale] !== "string" || translations[locale].trim() === "") fail(`native.json: infoPlist ${key} has no ${locale}`);
	}
	for (const locale of Object.keys(translations)) {
		if (!LOCALES.includes(locale)) fail(`native.json: infoPlist ${key} gives ${JSON.stringify(locale)}, which is not a language the apps speak`);
	}
	for (const locale of LOCALES) {
		if (placeholders(translations[locale] ?? "").length > 0) fail(`native.json: infoPlist ${key} in ${locale} has a placeholder, which the system would show as it is`);
	}
	const said = [...project.matchAll(new RegExp(`^\\s*${key}:\\s*(.+?)\\s*$`, "gm"))].map((match) => match[1]);
	if (said.length === 0) fail(`project.yml gives no ${key}, which native.json translates`);
	for (const english of said) {
		if (english !== translations.en) fail(`project.yml gives ${key} as ${JSON.stringify(english)}, and native.json's English is ${JSON.stringify(translations.en)}`);
	}
}
for (const match of project.matchAll(/^\s*(NS\w+UsageDescription):/gm)) {
	if (!(match[1] in infoPlist)) fail(`project.yml asks with ${match[1]}, which native.json's infoPlist does not translate`);
}
stop();

/* Every phrase, in every language, from wherever it came. */

const phrases = new Map();
for (const phrase of english) {
	phrases.set(phrase, Object.fromEntries(LOCALES.map((locale) => [locale, web[locale][phrase]])));
}
for (const [phrase, translations] of Object.entries(nativePhrases)) {
	phrases.set(phrase, translations);
}

/*
 * A placeholder translated along with the sentence around it, `{name}` becoming
 * `{名前}`, puts the braces on screen in a language whoever checks the build
 * may not read. The web checks the three translations; English is checked here
 * as well, because some of its values differ from their keys ("cn-east" reads
 * "China East").
 */
for (const [phrase, translations] of phrases) {
	const expected = placeholders(phrase).join(",");
	for (const locale of LOCALES) {
		const found = placeholders(translations[locale]).join(",");
		if (found !== expected) {
			fail(`${locale}: ${JSON.stringify(phrase)} carries {${found}} where the English carries {${expected}}`);
		}
	}
}
stop();

/*
 * The catalog, in the layout Xcode writes. Entries are kept as arrays of pairs
 * rather than objects, because JavaScript moves integer-like keys to the front
 * of an object whatever order they were added in, and a phrase such as "2"
 * would then sort differently from how it was asked to.
 */
const byCodeUnit = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

const strings = [...phrases.keys()].sort(byCodeUnit).map((phrase) => [
	phrase,
	[
		// Manual, so that Xcode, finding no call it recognises in the source,
		// leaves the entry alone rather than marking it stale.
		["extractionState", "manual"],
		// And without a generated symbol. The build makes one Swift property
		// per manual string, named from the key, and stops when two keys make
		// the same name: "Rooms" and "rooms" are both phrases. The properties
		// would also be a second way to say a phrase, in the system's language
		// rather than the one chosen, going around t().
		["generatesSymbol", false],
		[
			"localizations",
			[...LOCALES].sort(byCodeUnit).map((locale) => [
				locale,
				[["stringUnit", [["state", "translated"], ["value", phrases.get(phrase)[locale]]]]],
			]),
		],
	],
]);

const catalog = [
	["sourceLanguage", "en"],
	["strings", strings],
	["version", "1.0"],
];

function render(value, indent) {
	if (typeof value === "string" || typeof value === "boolean") return JSON.stringify(value);
	if (value.length === 0) return "{\n\n" + indent + "}";
	const inner = `${indent}  `;
	const lines = value.map(([key, child]) => `${inner}${JSON.stringify(key)} : ${render(child, inner)}`);
	return `{\n${lines.join(",\n")}\n${indent}}`;
}

const infoPlistCatalog = [
	["sourceLanguage", "en"],
	[
		"strings",
		Object.keys(infoPlist).sort(byCodeUnit).map((key) => [
			key,
			[
				["extractionState", "manual"],
				["generatesSymbol", false],
				[
					"localizations",
					[...LOCALES].sort(byCodeUnit).map((locale) => [
						locale,
						[["stringUnit", [["state", "translated"], ["value", infoPlist[key][locale]]]]],
					]),
				],
			],
		]),
	],
	["version", "1.0"],
];

const outputs = [
	[catalogPath, `${render(catalog, "")}\n`,
		`${phrases.size} phrases (${english.length} from the web, ${phrases.size - english.length} native) in ${LOCALES.length} languages`],
	[infoPlistPath, `${render(infoPlistCatalog, "")}\n`, `${Object.keys(infoPlist).length} Info.plist strings in ${LOCALES.length} languages`],
];

if (process.argv.includes("--check")) {
	let stale = false;
	for (const [path, output, counts] of outputs) {
		const shown = relative(process.cwd(), path);
		const current = existsSync(path) ? readFileSync(path, "utf8") : "";
		if (current !== output) {
			console.error(`generate.mjs: ${shown} is out of date; run node apple/i18n/generate.mjs`);
			stale = true;
		} else {
			console.log(`${shown} is up to date: ${counts}.`);
		}
	}
	if (stale) process.exit(1);
} else {
	for (const [path, output, counts] of outputs) {
		writeFileSync(path, output);
		console.log(`Wrote ${relative(process.cwd(), path)}: ${counts}.`);
	}
}
