/*
 * Fixtures for the lossless stream, made by the web client's own code.
 *
 * The Rust port in core/protocol/src/lossless/ has to write the bytes a browser
 * writes and read a browser's packets to the samples a browser reads. That is
 * not something to reason about; these files are what the web client does, and
 * core/protocol/tests/lossless_fixtures.rs holds the port to them.
 *
 * Regenerate from this directory, with Node 25 (any Node that has
 * --experimental-transform-types and module.registerHooks):
 *
 *     node --experimental-transform-types generate.mjs [path/to/tomoshibi-relay/web]
 *
 * The web directory defaults to $TOMOSHIBI_WEB, then to the server checkout
 * beside this repository. Its node_modules must be installed: lossless.ts
 * imports livekit-client for the track sources. Transform rather than plain
 * type stripping, because lossless.ts has a constructor parameter property,
 * which stripping refuses; and the web files import each other without the
 * .ts extension, as Vite allows, so a resolve hook below adds it.
 *
 * Nothing here depends on the clock or on chance: the signals come from a
 * seeded generator, the sender's stream ids from a fixed list. Running it twice
 * writes the same files, and a change in them is a change in the web client.
 *
 * What is written:
 *
 * - blocks.json: blocks of every character, packed by the web client. Each
 *   case's samples are both what was packed and what the web client's unpack
 *   returned for the packet; the generator stops if those ever differ.
 * - malformed.json: packets cut short, bit-flipped and invented, and what the
 *   web client's unpack made of each: its error message, or the samples it
 *   returned, wrapped as an Int32Array wraps them. A cut is kept as the name
 *   of its whole packet, under "bases", and a length.
 * - conversions.json: floats through toInt24 and decodedToInt24(16), as the
 *   sender stores them.
 * - sender.json: the web client's LosslessSender itself, fed capture through a
 *   stand-in for MediaStreamTrackProcessor and a stand-in room, with what it
 *   published after each step.
 *
 * Binary data is base64: samples as little-endian Int32, floats as
 * little-endian Float32, so every value crosses exactly.
 */

import { writeFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const web = resolve(process.argv[2] ?? process.env.TOMOSHIBI_WEB ?? join(here, "../../../../../../tomoshibi-relay/web"));
const live = pathToFileURL(join(web, "src/live/")).href;

registerHooks({
	resolve(specifier, context, next) {
		try {
			return next(specifier, context);
		} catch (err) {
			if (specifier.startsWith(".") && !/\.[cm]?[jt]s$/.test(specifier)) return next(`${specifier}.ts`, context);
			throw err;
		}
	},
});

const lossless = await import(new URL("lossless.ts", live).href);
const { pack, unpack, toInt24, decodedToInt24, Malformed, FRAMES, SHARED, MUSIC } = lossless;

const TOP = 8_388_607;
const BOTTOM = -8_388_608;

/** The web tests' own generator, so a case here is the case there. */
function random(seed) {
	let state = seed >>> 0;
	return () => {
		state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
		return state / 2 ** 32;
	};
}

const b64 = (bytes) => Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength).toString("base64");
const ints = (array) => b64(Int32Array.from(array));
const floats = (array) => b64(Float32Array.from(array));
const frames = (make, n = FRAMES) => Array.from({ length: n }, (_, i) => make(i));
const clip = (v) => Math.max(BOTTOM, Math.min(TOP, v));

function write(name, value) {
	writeFileSync(join(here, name), `${JSON.stringify(value, null, "\t")}\n`);
	console.log(`wrote ${name}`);
}

/* blocks.json */

const cases = [];

function block(name, channels, { rate = 48_000, stream = 0xdeadbeef, seq = 7, extra = {} } = {}) {
	const input = { rate, stream, seq, channels: channels.map((c) => Int32Array.from(c)) };
	const packet = pack(input);
	const output = unpack(packet);
	const same =
		output.rate === rate &&
		output.stream === stream &&
		output.seq === seq &&
		output.channels.length === input.channels.length &&
		output.channels.every((c, i) => c.length === input.channels[i].length && c.every((v, j) => v === input.channels[i][j]));
	if (!same) throw new Error(`${name}: the web client did not get back what it packed`);
	cases.push({ name, rate, stream, seq, samples: output.channels.map(ints), packet: b64(packet), ...extra });
}

const sine = (amplitude, hz, rate = 48_000, phase = 0) => (i) => Math.round(amplitude * Math.sin((2 * Math.PI * hz * i) / rate + phase));

block("silence", [frames(() => 0), frames(() => 0)]);
block("sine-1k-mono", [frames(sine(0.8 * TOP, 1000))]);
block("stereo-apart", [frames(sine(0.7 * TOP, 1000)), frames(sine(0.5 * TOP, 1500, 48_000, Math.PI / 2))]);
{
	const left = frames((i) => Math.round(3_000_000 * Math.sin(i / 7) + 800_000 * Math.sin(i / 1.3)));
	block("stereo-alike", [left, left.map((v, i) => v + Math.round(40_000 * Math.sin(i / 3)))]);
}
block("identical-channels", [frames(sine(0.6 * TOP, 440)), frames(sine(0.6 * TOP, 440))]);
{
	const next = random(1);
	const noise = () => Math.floor(next() * (TOP - BOTTOM + 1)) + BOTTOM;
	block("noise", [frames(noise), frames(noise)]);
}
block("square-1k", [frames((i) => (i % 48 < 24 ? TOP : BOTTOM)), frames((i) => (i % 48 < 24 ? BOTTOM : TOP))]);
{
	const rail = (i) => (i % 2 ? TOP : BOTTOM);
	block("nyquist-rails", [frames(rail), frames((i) => rail(i + 1))]);
}
block("spike", [frames((i) => (i === 500 ? TOP : 0)), frames((i) => (i === 501 ? BOTTOM : 0))]);
{
	const next = random(6);
	const noise16 = () => (Math.floor(next() * 65_536) - 32_768) * 256;
	block("sixteen-bit-noise", [frames(noise16), frames(noise16)]);
}
{
	// A 16-bit file as Chrome hands it over: negative over 2^15, positive over
	// 2^15 - 1, to a 32-bit float. The floats go in the case too, so the port
	// recovers the integers itself before packing them.
	const file = [frames(sine(30_000, 997)), frames(sine(32_767, 1234, 48_000, 1))];
	file[1][3] = -32_768;
	file[1][4] = 32_767;
	const chrome = file.map((c) => c.map((v) => Math.fround(v < 0 ? v / 32_768 : v / 32_767)));
	const recover = decodedToInt24(16);
	const recovered = chrome.map((c) => c.map((f) => recover(f)));
	recovered.forEach((c, k) => c.forEach((v, i) => {
		if (v !== file[k][i] * 256) throw new Error("the 16-bit path lost a sample");
	}));
	block("sixteen-bit-path", recovered, { extra: { floats: chrome.map(floats) } });
}
{
	const left = frames((i) => Math.round(2_000_000 * Math.sin(i / 9) + 300_000 * Math.sin(i / 2.1)), 333);
	block("short-final-333", [left, left.map((v, i) => v + Math.round(20_000 * Math.sin(i / 4)))]);
}
{
	const next = random(3);
	for (const n of [1, 2, 3, 4, 5, 9, 63, 64, 65, 961]) {
		const sample = () => Math.floor(next() * (TOP - BOTTOM + 1)) + BOTTOM;
		block(`noise-${n}`, [frames(sample, n), frames(sample, n)]);
		block(`smooth-${n}`, [frames(sine(0.5 * TOP, 200), n), frames(sine(0.4 * TOP, 300), n)]);
	}
}
block("rate-44100", [frames(sine(0.8 * TOP, 1000, 44_100)), frames(sine(0.8 * TOP, 1000, 44_100, 0.3))], { rate: 44_100 });
{
	const next = random(2);
	block("mono-noise", [frames(() => Math.floor(next() * 2_000_000) - 1_000_000)]);
}
block("rails-top", [frames(() => TOP), frames(() => TOP)]);
block("rails-bottom", [frames(() => BOTTOM)]);
block("widest-side", [frames(() => TOP), frames(() => BOTTOM)]);
block("dc", [frames(() => 123_456), frames(() => -654_321)]);
block("ramp", [frames((i) => i * 8000 - 3_840_000), frames((i) => 3_840_000 - i * 8000)]);
block("cubic", [frames((i) => Math.round(((i - 480) ** 3) / 20)), frames((i) => Math.round(((i - 300) ** 2) * 7))]);
for (const shift of [1, 4, 8, 12, 20, 23]) {
	const next = random(100 + shift);
	const step = 2 ** shift;
	const span = Math.floor((TOP + 1) / step);
	const make = () => (Math.floor(next() * 2 * span) - span) * step;
	block(`wasted-${shift}`, [frames(make), frames(make)]);
}
block("long-4099", [frames(sine(0.9 * TOP, 50), 4099)]);
block("sequence-numbers", [frames(sine(1000, 1000), 10)], { stream: 0, seq: 0xffffffff });
block("header-values", [frames(sine(1000, 1000), 10)], { rate: 0xffffffff, stream: 0x01020304, seq: 0x80000000 });
{
	// The web tests' thousand random blocks, the first twenty of them.
	const next = random(4);
	for (let round = 0; round < 20; round++) {
		const scale = 2 ** Math.floor(next() * 24);
		const shape = Math.floor(next() * 3);
		const make = (i) => {
			const v =
				shape === 0
					? Math.floor((next() - 0.5) * scale)
					: shape === 1
						? Math.round(scale * Math.sin(i / (1 + next() * 50)))
						: (i % 97 === 0 ? 1 : 0) * Math.floor((next() - 0.5) * 2 * TOP);
			return clip(v);
		};
		const n = 1 + Math.floor(next() * 1200);
		const stereo = next() < 0.8;
		block(`random-${round}`, stereo ? [frames(make, n), frames(make, n)] : [frames(make, n)], { seq: round });
	}
}

write("blocks.json", { cases });

/* malformed.json */

const broken = [];

/** Records what the web client made of `bytes`; `as` says how to find them again. */
function attempt(name, bytes, as = { packet: b64(bytes) }) {
	try {
		const out = unpack(bytes);
		broken.push({
			name,
			...as,
			error: null,
			rate: out.rate,
			stream: out.stream,
			seq: out.seq,
			samples: out.channels.map(ints),
		});
	} catch (err) {
		if (!(err instanceof Malformed)) throw err;
		broken.push({ name, ...as, error: err.message });
	}
}

const left70 = frames((i) => Math.round(2_000_000 * Math.sin(i / 5)), 70);
const bases = {
	stereo: pack({ rate: 48_000, stream: 5, seq: 1, channels: [Int32Array.from(left70), Int32Array.from(left70, (v, i) => v - i * 100)] }),
	mono: pack({ rate: 48_000, stream: 6, seq: 2, channels: [Int32Array.from(frames((i) => (i % 7) * 1000 - 3000, 20))] }),
	wasted: pack({ rate: 44_100, stream: 7, seq: 3, channels: [Int32Array.from(frames((i) => ((i * 37) % 200 - 100) * 256, 66))] }),
	escape: pack({ rate: 48_000, stream: 8, seq: 4, channels: [Int32Array.from(frames((i) => (i % 2 ? TOP : BOTTOM), 64))] }),
};

{
	const next = random(9);

	for (const [base, bytes] of Object.entries(bases)) {
		attempt(`${base}-whole`, bytes);
		for (let length = 0; length < bytes.length; length++) {
			attempt(`${base}-cut-${length}`, bytes.slice(0, length), { base, length });
		}
		attempt(`${base}-trailing`, Uint8Array.from([...bytes, 0xff, 0x00, 0x12]));

		for (let n = 0; n < 60; n++) {
			const copy = bytes.slice();
			const flips = 1 + Math.floor(next() * 3);
			for (let f = 0; f < flips; f++) {
				const bit = 16 * 8 + Math.floor(next() * (copy.length - 16) * 8);
				copy[bit >> 3] ^= 0x80 >> (bit & 7);
			}
			attempt(`${base}-flip-${n}`, copy);
		}

		for (let n = 0; n < 20; n++) {
			const copy = bytes.slice(0, 16);
			const body = Uint8Array.from({ length: Math.floor(next() * 80) }, () => Math.floor(next() * 256));
			attempt(`${base}-invented-${n}`, Uint8Array.from([...copy, ...body]));
		}
	}

	const header = (version, channels, count) => {
		const bytes = new Uint8Array(16 + 64);
		bytes[0] = version;
		bytes[1] = channels;
		bytes[2] = count & 0xff;
		bytes[3] = count >> 8;
		return bytes;
	};
	attempt("version-0", header(0, 1, 10));
	attempt("version-2", header(2, 1, 10));
	attempt("channels-0", header(1, 0, 10));
	attempt("channels-3", header(1, 3, 10));
	attempt("frames-0", header(1, 1, 0));
	attempt("frames-65535-zeros", header(1, 1, 65_535));
	{
		// Wasted bits past 24: five bits of 25 (11001), then order 0.
		const bytes = header(1, 1, 4);
		bytes[16] = 0b11001000;
		attempt("wasted-25", bytes);
	}
	{
		// Order 5: five bits of 0, then 101.
		const bytes = header(1, 1, 8);
		bytes[16] = 0b00000101;
		attempt("order-5", bytes);
	}
	{
		// Order 4 in a 3-frame block.
		const bytes = header(1, 1, 3);
		bytes[16] = 0b00000100;
		attempt("order-past-frames", bytes);
	}
	{
		// A warm-up of 257 shifted up by 24, which leaves 32 bits: the
		// browser's Int32Array wraps it. Shift 24 (11000), order 1 (001), the
		// warm-up as 2^26 + 257 in 27 bits, then a Rice parameter of 0 for a
		// partition with nothing in it.
		const bytes = header(1, 1, 1);
		bytes.set([0b11000001, 0x80, 0x00, 0x20, 0x20], 16);
		attempt("warmup-wraps", bytes);
	}
	{
		// A unary run longer than any residual this coding writes.
		const bytes = new Uint8Array(16 + 9_000_000);
		bytes[0] = 1;
		bytes[1] = 1;
		bytes[2] = 1;
		// Shift 0, order 0, Rice parameter 0, then zeros all the way.
		attempt("runaway", bytes);
		// Too large to keep; the result is what matters.
		broken.at(-1).packet = null;
		broken.at(-1).note = "16 bytes of header (version 1, one channel, one frame) and 9000000 zero bytes";
	}
}

write("malformed.json", { bases: Object.fromEntries(Object.entries(bases).map(([name, bytes]) => [name, b64(bytes)])), cases: broken });

/* conversions.json */

{
	const values = [];
	const add = (f) => values.push(Math.fround(f));
	for (let k = -40; k <= 40; k++) add(k / 2 ** 24);
	for (let k = -40; k <= 40; k++) add((k + 0.25) / 2 ** 23);
	for (const v of [0, -0, 1, -1, 1.5, -1.5, 2, -2, 0.5, -0.5, 1 - 2 ** -24, -1 + 2 ** -24, 2 ** -149, -(2 ** -149), 3.4e38, -3.4e38]) add(v);
	for (const v of [Infinity, -Infinity, NaN]) add(v);
	for (const v of [1000.6, 1000.4, -1000.6, -1000.4, 1000.5, -1000.5, 8_388_607.5, -8_388_608.5]) add(v / 8_388_608);
	for (let v = -32_768; v <= 32_767; v += 97) add(v < 0 ? v / 32_768 : v / 32_767);
	for (const v of [-32_768, -32_767, -1, 0, 1, 6827, 32_766, 32_767]) add(v < 0 ? v / 32_768 : v / 32_767);
	const next = random(11);
	const bits = new Uint32Array(1);
	const asFloat = new Float32Array(bits.buffer);
	for (let n = 0; n < 600; n++) {
		bits[0] = Math.floor(next() * 2 ** 32);
		values.push(asFloat[0]);
	}
	for (let n = 0; n < 300; n++) add((next() - 0.5) * 2.2);

	const sixteen = decodedToInt24(16);
	// What the sender stores: whatever the function returns, into an Int32Array.
	const stored = (v) => Int32Array.of(v)[0];
	const table = values.map((f) => {
		bits[0] = 0;
		asFloat[0] = f;
		return [bits[0], stored(toInt24(f)), stored(sixteen(f))];
	});
	write("conversions.json", { columns: ["float32 bits", "toInt24", "decodedToInt24(16)"], values: table });
}

/* sender.json */

const ids = [];
Object.defineProperty(Object.getPrototypeOf(globalThis.crypto), "getRandomValues", {
	configurable: true,
	writable: true,
	value(array) {
		const id = ids.shift();
		if (id === undefined) throw new Error("ran out of stream ids");
		array[0] = id;
		drawn.push(id);
		return array;
	},
});
let drawn = [];

let feed = () => {};
globalThis.MediaStreamTrackProcessor = class {
	constructor() {
		this.readable = new ReadableStream({
			start(controller) {
				feed = (value) => {
					try {
						controller.enqueue(value);
					} catch {}
				};
			},
		});
	}
};

const { LosslessSender } = await import(new URL("lossless-sender.ts", live).href);

const settle = () => new Promise((done) => setTimeout(done, 0));

function fakeRoom(stalled) {
	const handlers = new Map();
	const remoteParticipants = new Map();
	const published = [];
	const room = {
		remoteParticipants,
		localParticipant: {
			publishData(bytes, options) {
				published.push({ topic: options.topic, to: [...options.destinationIdentities], payload: b64(bytes) });
				return new Promise((done) => {
					if (!stalled) done();
				});
			},
		},
		on(event, handler) {
			if (!handlers.has(event)) handlers.set(event, new Set());
			handlers.get(event).add(handler);
		},
		off(event, handler) {
			handlers.get(event)?.delete(handler);
		},
	};
	const emit = (event, ...args) => {
		for (const handler of handlers.get(event) ?? []) handler(...args);
	};
	return { room, remoteParticipants, published, emit };
}

/**
 * Run a script of steps through a real LosslessSender and record what it
 * published after each. A step is a join, a silent departure, an ask, or a
 * chunk of capture with its planes as floats.
 */
async function run(name, steps, { channel = SHARED, bits, stalled = false } = {}) {
	ids.length = 0;
	ids.push(0x11111111, 0x22222222, 0x33333333, 0x44444444, 0x55555555);
	drawn = [];
	const { room, remoteParticipants, published, emit } = fakeRoom(stalled);
	let gaveUp = 0;
	const sender = new LosslessSender(room, {}, () => gaveUp++, bits === undefined ? toInt24 : decodedToInt24(bits), channel);
	await settle();

	const recorded = [];
	for (const step of steps) {
		const before = published.length;
		const upBefore = gaveUp;
		if (step.join) remoteParticipants.set(step.join, { identity: step.join });
		if (step.vanish) remoteParticipants.delete(step.vanish);
		if (step.ask) {
			const { from, topic, payload } = step.ask;
			emit("dataReceived", Uint8Array.from(Buffer.from(payload, "base64")), remoteParticipants.get(from), 0, topic);
		}
		if (step.chunk) {
			const { rate, timestamp, planes } = step.chunk;
			const arrays = planes.map((p) => (typeof p === "number" ? new Float32Array(p) : Float32Array.from(p)));
			feed({
				sampleRate: rate,
				numberOfChannels: arrays.length,
				numberOfFrames: arrays[0].length,
				timestamp,
				copyTo(dest, options) {
					dest.set(arrays[options.planeIndex]);
				},
				close() {},
			});
		}
		await settle();
		await settle();
		const out = { ...step, published: published.slice(before), gaveUp: gaveUp > upBefore };
		if (step.chunk) {
			out.chunk = {
				rate: step.chunk.rate,
				timestamp: step.chunk.timestamp,
				planes: step.chunk.planes.map((p) => (typeof p === "number" ? { silence: p } : floats(p))),
			};
		}
		recorded.push(out);
	}

	const result = { name, channel: channel.sound, bits: bits ?? null, stalled, streams: drawn.slice(1), gaps: sender.gaps, gaveUp, steps: recorded };
	sender.stop();
	return result;
}

const yes = Buffer.from("1").toString("base64");
const no = Buffer.from("0").toString("base64");
const bomYes = Buffer.from([0xef, 0xbb, 0xbf, 0x31]).toString("base64");

/**
 * Capture whose floats are k / 2^24 with k odd half the time: exactly between
 * two 24-bit samples, which is where JavaScript's Math.round and Rust's round
 * part company below zero.
 */
function capture(rate, count, from, frames, hz = 997) {
	return Array.from({ length: count }, (_, c) =>
		Array.from({ length: frames }, (_, i) => {
			const s = from + i;
			const k = 2 * Math.round(2 ** 22 * 0.9 * Math.sin((2 * Math.PI * hz * s) / rate + c)) + ((Math.imul(s + 1, 2_654_435_761) >>> (13 + c)) & 1);
			return k / 2 ** 24;
		}),
	);
}

const runs = [];
{
	const steps = [{ join: "gfriend-1" }, { join: "gother-2" }];
	let t = 0;
	let frame = 0;
	const chunk = (rate, count, size, hole = false) => {
		if (hole) t += 10_000;
		steps.push({ chunk: { rate, timestamp: Math.round(t), planes: capture(rate, count, frame, size) } });
		t += (size / rate) * 1_000_000;
		frame += size;
	};
	chunk(48_000, 2, 480);
	chunk(48_000, 2, 480); // a block, made for nobody: its number is still taken
	steps.push({ ask: { from: "gfriend-1", topic: SHARED.ask, payload: yes } });
	for (let n = 0; n < 4; n++) chunk(48_000, 2, 480, n === 2);
	steps.push({ ask: { from: "gother-2", topic: SHARED.ask, payload: bomYes } });
	steps.push({ ask: { from: "gfriend-1", topic: MUSIC.ask, payload: no } }); // another channel's topic: ignored
	chunk(48_000, 3, 1000); // three planes: two are sent
	chunk(48_000, 2, 128);
	chunk(48_000, 2, 812);
	steps.push({ ask: { from: "gfriend-1", topic: SHARED.ask, payload: yes } }); // a repeat keeps its place
	chunk(48_000, 2, 960);
	steps.push({ vanish: "gfriend-1" });
	chunk(48_000, 2, 960);
	steps.push({ join: "gfriend-1" });
	steps.push({ ask: { from: "gfriend-1", topic: SHARED.ask, payload: yes } });
	steps.push({ ask: { from: "gother-2", topic: SHARED.ask, payload: no } });
	t = 0;
	frame = 0;
	for (let n = 0; n < 6; n++) chunk(44_100, 2, 441); // a new rate: a new stream
	t = 0;
	for (let n = 0; n < 3; n++) chunk(44_100, 1, 640); // a new channel count, the 44.1 kHz remainder dropped
	runs.push(await run("captured", steps));
}
{
	const file = (from, size) =>
		[0, 1].map((c) =>
			Array.from({ length: size }, (_, i) => {
				const v = Math.max(-32_768, Math.min(32_767, Math.round(32_767 * Math.sin((from + i) / (11 + c * 3)) * (c ? 1.01 : 1))));
				return v < 0 ? v / 32_768 : v / 32_767;
			}),
		);
	const steps = [
		{ join: "gfriend-1" },
		{ ask: { from: "gfriend-1", topic: SHARED.ask, payload: yes } }, // the shared sound's topic: ignored by the music
		{ chunk: { rate: 44_100, timestamp: 0, planes: file(0, 960) } },
		{ ask: { from: "gfriend-1", topic: MUSIC.ask, payload: yes } },
		{ chunk: { rate: 44_100, timestamp: 21_769, planes: file(960, 960) } },
		{ chunk: { rate: 44_100, timestamp: 43_537, planes: file(1920, 960) } },
	];
	runs.push(await run("music-sixteen-bit", steps, { channel: MUSIC, bits: 16 }));
}
{
	// An upload that never finishes a send: the first packet is in flight for
	// good, and the rest wait behind it until the sender gives up.
	const steps = [{ join: "gfriend-1" }, { ask: { from: "gfriend-1", topic: SHARED.ask, payload: yes } }];
	for (let n = 0; n < 160; n++) steps.push({ chunk: { rate: 48_000, timestamp: n * 20_000, planes: [960, 960] } });
	runs.push(await run("stalled-upload", steps, { stalled: true }));
}

write("sender.json", { runs });
