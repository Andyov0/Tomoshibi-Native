// What a browser receives of the probe's shared screen at each size it is
// drawn at: follows the probe into its room by invitation, waits for a share,
// then draws the page at a large, a middle and a small size in turn, so that
// adaptive streaming asks the media server for each rung in turn, and reports
// the picture decoded at each (its size, its rate, the decoder). The rungs
// themselves are the publisher's; this is the check that every one of them
// arrives at the size and rate the web publishes it at.
//   node share-layers.mjs <port> <base> <room> <invite> [shot.png]
const [port, BASE, ROOM, INVITE, SHOT] = process.argv.slice(2);
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
import { writeFileSync } from "node:fs";

function cdp(port) {
	let ws, id = 0;
	const send = (m, params = {}) => new Promise((ok, bad) => {
		const mine = ++id;
		const on = (e) => { const x = JSON.parse(e.data); if (x.id !== mine) return; ws.removeEventListener("message", on); x.error ? bad(new Error(JSON.stringify(x.error))) : ok(x.result); };
		ws.addEventListener("message", on); ws.send(JSON.stringify({ id: mine, method: m, params }));
	});
	return {
		async open() {
			const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
			const target = list.find((t) => t.type === "page");
			ws = await new Promise((ok, bad) => { const s = new WebSocket(target.webSocketDebuggerUrl); s.onopen = () => ok(s); s.onerror = bad; });
			await send("Page.enable"); await send("Runtime.enable"); await send("DOM.enable");
		},
		go: (url) => send("Page.navigate", { url }),
		send,
		async run(e) { const r = await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true }); return r.exceptionDetails ? "EXC:" + JSON.stringify(r.exceptionDetails).slice(0, 300) : r.result.value; },
		close: () => ws.close(),
	};
}
async function type(page, selector, text) {
	const doc = await page.send("DOM.getDocument", {});
	const f = await page.send("DOM.querySelector", { nodeId: doc.root.nodeId, selector });
	if (!f.nodeId) return "not found";
	await page.send("DOM.focus", { nodeId: f.nodeId });
	for (const ch of text) { await page.send("Input.dispatchKeyEvent", { type: "keyDown", text: ch }); await page.send("Input.dispatchKeyEvent", { type: "keyUp" }); }
	return "ok";
}

// The received video tracks, each with what was decoded over the last two
// seconds: the largest is the share (a phone's camera is never this large).
const SAMPLE = `(async () => {
	const read = async () => {
		const out = [];
		for (const pc of window.__pcs) {
			const stats = await pc.getStats();
			const codecs = {}; stats.forEach((s) => { if (s.type === "codec") codecs[s.id] = s.mimeType; });
			stats.forEach((s) => { if (s.type === "inbound-rtp" && s.kind === "video") out.push({ id: s.id, frames: s.framesDecoded || 0, w: s.frameWidth, h: s.frameHeight, fps: s.framesPerSecond, decoder: s.decoderImplementation, codec: codecs[s.codecId], at: s.timestamp }); });
		}
		return out;
	};
	const a = await read(); await new Promise((r) => setTimeout(r, 2000)); const b = await read();
	return JSON.stringify(b.map((x) => { const y = a.find((z) => z.id === x.id); const decoded = y ? ((x.frames - y.frames) * 1000) / (x.at - y.at) : null; return { size: x.w + "x" + x.h, decodedPerSecond: decoded && Math.round(decoded * 10) / 10, reported: x.fps, decoder: x.decoder, codec: x.codec }; }).filter((x) => x.size !== "undefinedxundefined"));
})()`;

const page = cdp(port);
await page.open();
await page.go("about:blank");
await page.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
	const Original = window.RTCPeerConnection; window.__pcs = [];
	function Hooked(...a) { const pc = new Original(...a); window.__pcs.push(pc); return pc; }
	Hooked.prototype = Original.prototype; Object.setPrototypeOf(Hooked, Original);
	window.RTCPeerConnection = Hooked;
})()` });
const sizes = [["large", 2560, 1600], ["middle", 1280, 800], ["small", 560, 360]];
await page.send("Emulation.setDeviceMetricsOverride", { width: sizes[0][1], height: sizes[0][2], deviceScaleFactor: 1, mobile: false });
await page.go(`${BASE}/robots.txt`); await wait(800);
await page.run("localStorage.clear(); sessionStorage.clear(); 'cleared'");
await page.go("about:blank"); await wait(300);
await page.go(`${BASE}/?invite=${INVITE}#/${ROOM}`);
await wait(3500);
console.log("name:", await type(page, 'input[aria-label="Your name"]', "Web Peer"));
await wait(300);
console.log("join:", await page.run(`(() => { const b = [...document.querySelectorAll("button")].find(x => /join/i.test(x.textContent)); if (!b) return "no join button"; if (b.disabled) return "disabled"; b.click(); return "clicked"; })()`));
let shared = false;
for (let i = 0; i < 90 && !shared; i++) {
	await wait(1000);
	const now = JSON.parse(await page.run(SAMPLE));
	shared = now.some((v) => v.decodedPerSecond > 0 && Number(v.size.split("x")[0]) >= 640);
}
console.log("share arrived:", shared);
for (const [name, width, height] of sizes) {
	await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
	// Adaptive streaming waits for the element to settle, the server for a
	// key frame on the rung it switches to.
	await wait(9000);
	console.log(`${name} ${width}x${height}:`, await page.run(SAMPLE));
	if (name === "large" && SHOT) {
		const shot = await page.send("Page.captureScreenshot", { format: "png" });
		writeFileSync(SHOT, Buffer.from(shot.data, "base64"));
	}
}
page.close();
process.exit(0);
